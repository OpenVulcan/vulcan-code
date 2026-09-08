import type { PublicAiInferenceRoute, PublicAiEmbeddingCapabilities } from "./public-ai";

// Explicit human compatibility decision; dimensions can never be overridden.
// 明确人为兼容决定；维度绝不能被覆盖。
export type VceEmbeddingDecision = "compatible" | "rebuild";

// Candidate inspected through the common model registry, not activated by preview.
// 通过公共模型注册表检查的候选，预览不会激活它。
export interface VceEmbeddingCandidate {
  // Exact route selected in the common model filter.
  // 通用模型过滤器选择的精确路由。
  route: PublicAiInferenceRoute;
  // Exact explicit dimension suggested or entered by the user.
  // 建议或用户输入的精确明确维度。
  dimension: number;
  // Actual target identity; original indexes preserve their own fingerprints.
  // 实际目标身份；原始索引保留自己的指纹。
  physicalFingerprint: string;
  // Declared model constraints, not inferred provider defaults.
  // 已声明模型约束，不是推测的供应商默认值。
  capabilities: PublicAiEmbeddingCapabilities;
}

// Immutable original index scope serialized by the shared persisted policy type.
// 由共享持久化策略类型序列化的不可变原始索引作用域。
export interface VceEmbeddingScope {
  // Exact host owner, not a display name.
  // 精确宿主所有者，不是展示名称。
  project_id: string;
  // Exact original canonical root digest.
  // 精确原始规范根摘要。
  root_hash: string;
  // Exact namespace and checkpoint chain.
  // 精确命名空间及检查点链。
  namespace_id: string;
  checkpoint_chain_id: string;
  // Original physical identity, format, and dimensions.
  // 原始物理身份、格式及维度。
  original_fingerprint: string;
  index_schema_version: number;
  dimension: number;
  // Immutable source preprocessing and chunker versions.
  // 不可变源码预处理及切块器版本。
  preprocessing_version: string;
  chunker_version: string;
}

// Read-only impact of a draft embedding configuration on an existing project index.
// 草稿向量配置对现有项目索引的只读影响。
export interface VceEmbeddingProjectPreflight {
  // Human project title, excluded from compatibility identity.
  // 人类可读项目标题，不参与兼容身份。
  projectName: string;
  // Exact live source facts covered by confirmation.
  // 确认覆盖的精确现存来源事实。
  impact: { scope: VceEmbeddingScope; checkpointVersion: number; sourceFingerprints: string[];
    confirmationRequired: string[]; rejectedSources: string[]; hardRebuildReason: string | null };
  // Derived complete direct coverage, not an independent switch.
  // 派生的完整直接覆盖，不是独立开关。
  compatible: boolean;
}

// Exact reviewed metadata bound to one optimistic save transaction.
// 与一个乐观保存事务绑定的精确已审查元数据。
export interface VceEmbeddingPreflight {
  // Null represents explicit removal, not a missing route fallback.
  // 空值表示明确移除，不是缺失路由兜底。
  candidate: VceEmbeddingCandidate | null;
  // Complete sorted committed-project impact list.
  // 完整排序的已提交项目影响列表。
  projects: VceEmbeddingProjectPreflight[];
  // Non-secret confirmation digest; stale values must be rechecked.
  // 非敏感确认摘要；过期值必须重新检查。
  revision: string;
}

// Explicit choices submitted alongside route and dimension, never in a later separate save.
// 与路由及维度同时提交的明确选择，绝不在稍后单独保存。
export interface VceEmbeddingSaveConfirmation {
  // Exact digest returned by read-only preflight.
  // 只读预检返回的精确摘要。
  preflightRevision: string;
  // Exact reviewed projects and their decisions.
  // 精确已审查项目及其决定。
  decisions: { projectId: string; decision: VceEmbeddingDecision }[];
}

// VceProjectLifecycleState identifies one host-owned code-semantic lifecycle.
// VceProjectLifecycleState 标识一项由宿主持有的代码语义生命周期。
export type VceProjectLifecycleState =
  | "uninitialized"
  | "disabled"
  | "starting"
  | "restoring"
  | "indexing"
  | "ready"
  | "degraded_optional_ai"
  | "blocked_configuration"
  | "rebuild_required"
  | "failed"
  | "stopping";

// VceIndexUpdateMode is a project policy shared across all conversations.
// VceIndexUpdateMode 是所有会话共享的项目策略。
export type VceIndexUpdateMode = "manual" | "automatic";

// VceJobSubmitRequest is a human command; retries reuse its key without repeating indexing.
// VceJobSubmitRequest 是人为命令；重试复用其键而不重复索引。
export interface VceJobSubmitRequest {
  // Exact previous durable job selected for retry, absent for an unrelated new command.
  // 选择重试的精确先前持久作业，无关新命令中缺失。
  retryOfJobId?: string | null;
  // Exact project selected in the host workbench.
  // 宿主工作台选择的精确项目。
  projectId: string;
  // Creation is explicitly distinct from synchronization.
  // 创建明确独立于同步。
  action: "initialize" | "synchronize";
  // Stable key for this button submission, including transport retries.
  // 本次按钮提交的稳定键，包括传输重试。
  idempotencyKey: string;
  // Observed persisted project settings revision.
  // 观察到的持久项目设置修订。
  expectedSettingsRevision: number;
  // Observed checkpoint, absent only before initialization.
  // 观察到的检查点，仅初始化前为空。
  expectedCheckpointVersion: number | null;
}

// VceJobSummary mirrors only durable host receipts, never calculated query eligibility.
// VceJobSummary 仅镜像持久宿主回执，绝不计算查询资格。
export interface VceJobSummary {
  // Explicit retry origin returned by the host; old receipts may omit it.
  // 宿主返回的明确重试来源；旧回执可能省略。
  retryOfJobId?: string | null;
  // Exact physical project's workbench owner.
  // 精确物理项目的工作台所有者。
  projectId: string;
  // Stable identity for progress, reconnect recovery, and cancellation.
  // 用于进度、重连恢复与取消的稳定身份。
  jobId: string;
  // Human-confirmed operation category.
  // 人为确认操作分类。
  kind: "initialize" | "synchronize" | "rebuild";
  // Durable execution state, independent from conversation state.
  // 持久执行状态，独立于会话状态。
  state: "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled" | "interrupted";
  // Monotonic receipt sequence used to reject reordered events.
  // 用于拒绝乱序事件的单调回执序号。
  sequence: number;
  // Wall-clock admission time for display, not a fabricated progress timer.
  // 用于显示的墙上时钟准入时间，而非虚构进度计时器。
  createdAtUnixMs: number;
  // Last durably acknowledged receipt time.
  // 最近持久确认回执时间。
  updatedAtUnixMs: number;
  // Prior checkpoint preserved during an ordinary update.
  // 普通更新期间保留的先前检查点。
  sourceCheckpointVersion: number | null;
  // Resulting checkpoint, present only for an acknowledged success.
  // 结果检查点，仅已确认成功时存在。
  resultCheckpointVersion: number | null;
  // Actual bounded phase counters, absent before source work begins.
  // 实际有界阶段计数，源码处理开始前为空。
  progress: {
    // Exact processing phase reported by the core.
    // 核心报告的精确处理阶段。
    phase: "scanning" | "reconciling" | "chunking" | "embedding" | "writing" | "committing" | "complete";
    // Exact operation owning these counters.
    // 持有这些计数的精确操作。
    operationId: string;
    // Meaning of the numeric counters.
    // 数字计数的含义。
    unit: "files" | "blobs" | "batches" | "none";
    // Confirmed completions, not started requests.
    // 已确认完成量，而非已开始请求量。
    completed: number;
    // Unknown totals stay indeterminate.
    // 未知总量保持不确定态。
    total: number | null;
    // Optional relative path, never source content.
    // 可选相对路径，绝不包含源码内容。
    currentPath: string | null;
  } | null;
  // Whether one subsequent manual sync was coalesced by the host.
  // 宿主是否合并一个后续手动同步。
  followupRequested: boolean;
  // Bounded execution/cleanup failure text.
  // 有界执行或清理失败文本。
  error: string | null;
}

// VceProjectSettingsSummary contains persisted settings plus host-owned metadata.
// VceProjectSettingsSummary 包含持久化设置与宿主持有的元数据。
export interface VceProjectSettingsSummary {
  enabled: boolean;
  // Exact project policy for admitting file-event updates.
  // 文件事件更新准入使用的精确项目策略。
  updateMode: VceIndexUpdateMode;
  // Minimum automatic admission interval; does not delay manual sync.
  // 自动受理的最低间隔；不延迟手动同步。
  autoUpdateMinIntervalSeconds: number;
  revision: number;
  migratedFromCodebaseMemory: boolean;
  maxFileBytes: number;
  extraIgnorePatterns: string[];
  allowQueryRewrite: boolean;
  allowRerank: boolean;
  embeddingMaxConcurrency: number;
  queryMaxConcurrency: number;
  maxResults: number;
  maxContextChars: number;
}

// VceProjectSettingsInput contains only user-editable project settings.
// VceProjectSettingsInput 只包含用户可编辑的项目设置。
export interface VceProjectSettingsInput {
  enabled: boolean;
  // Project-wide update policy selected by the user.
  // 用户选择的项目级更新策略。
  updateMode: VceIndexUpdateMode;
  // Explicit interval in seconds between automatic update admissions.
  // 两次自动更新受理之间的明确秒数间隔。
  autoUpdateMinIntervalSeconds: number;
  maxFileBytes: number;
  extraIgnorePatterns: string[];
  allowQueryRewrite: boolean;
  allowRerank: boolean;
  embeddingMaxConcurrency: number;
  queryMaxConcurrency: number;
  maxResults: number;
  maxContextChars: number;
}

// VceProjectStatusSummary is the bounded runtime state used by project surfaces.
// VceProjectStatusSummary 是项目界面使用的有界运行状态。
export interface VceProjectStatusSummary {
  projectId: string;
  enabled: boolean;
  settingsRevision: number;
  lifecycle: VceProjectLifecycleState;
  engineState?: string | null;
  workspaceGeneration?: number | null;
  checkpointVersion?: number | null;
  presentFiles: number;
  checkpointBlobs: number;
  lastError?: string | null;
}

// VceAiCapabilitySummary reports capability availability without provider credentials.
// VceAiCapabilitySummary 报告不含 Provider 凭据的能力可用性。
export interface VceAiCapabilitySummary {
  registered: boolean;
  embeddingAvailable: boolean;
  queryRewriteAvailable: boolean;
  rerankAvailable: boolean;
}

// VceConfigurationCapabilityStatus describes one required public-AI route.
// VceConfigurationCapabilityStatus 描述一条必需公共 AI 路由。
export interface VceConfigurationCapabilityStatus {
  configured: boolean;
  available: boolean;
  issue?: string | null;
}

// VceReasoningPolicy identifies the host-enforced reasoning mode for query rewriting.
// VceReasoningPolicy 标识宿主为查询改写强制执行的推理模式。
export type VceReasoningPolicy = "fixed_off";

// VceGlobalConfigurationStatus is the authoritative gate for every VCE project surface.
// VceGlobalConfigurationStatus 是每个 VCE 项目界面共同使用的权威闸门。
export interface VceGlobalConfigurationStatus {
  ready: boolean;
  consumerProfileId: string;
  queryRewritePurposeId: string;
  reasoningPolicy: VceReasoningPolicy;
  embedding: VceConfigurationCapabilityStatus;
  queryRewrite: VceConfigurationCapabilityStatus;
  rerank: VceConfigurationCapabilityStatus;
  issues: string[];
}

// VceIndexCountsSummary contains aggregate persisted index counts.
// VceIndexCountsSummary 包含持久化索引聚合计数。
export interface VceIndexCountsSummary {
  files: number;
  searchableFiles: number;
  skippedFiles: number;
  deletedFiles: number;
  blobs: number;
  chunks: number;
  vectors: number;
  pathDocuments: number;
  symbols: number;
}

// VceIndexIntegritySummary reports active-namespace consistency.
// VceIndexIntegritySummary 报告活动命名空间一致性。
export interface VceIndexIntegritySummary {
  indexExists: boolean;
  searchable: boolean;
  checkpointBlobCount: number;
  indexedBlobCount: number;
  missingBlobCount: number;
  orphanBlobCount: number;
  detail?: string | null;
}

// VceProjectOverviewResponse combines settings, runtime, AI, counts, and integrity.
// VceProjectOverviewResponse 组合设置、运行态、AI、计数与完整性。
export interface VceProjectOverviewResponse {
  // Absent on lightweight activity; content management requires the full overview's exact identity.
  // 轻量活动不包含此值；内容管理要求完整概览的精确身份。
  readIdentity?: string | null;
  configuration: VceGlobalConfigurationStatus;
  settings: VceProjectSettingsSummary;
  status: VceProjectStatusSummary;
  ai?: VceAiCapabilitySummary | null;
  counts?: VceIndexCountsSummary | null;
  integrity?: VceIndexIntegritySummary | null;
}

// Shared lightweight activity does not run management scans or lock a running semantic query.
// 共享轻量活动不执行管理扫描，也不锁定正在执行的语义查询。
export interface VceProjectActivityResponse {
  // Host observation evidence; independent from query admission and last successful publication.
  // 宿主观察证据；独立于查询准入及最近成功发布。
  sourceFreshness: {
    // Exact event consumer and physical registration are live.
    // 精确事件消费者及物理注册存活。
    watching: boolean;
    // Known delivered edits remain unsynchronized.
    // 已知投递编辑仍未同步。
    pendingChanges: boolean;
    // Next admitted update must reconcile the complete source manifest.
    // 下一次准入更新必须核对完整源码清单。
    reconciliationRequired: boolean;
  };
  // Authoritative global model gate.
  // 权威全局模型门禁。
  configuration: VceGlobalConfigurationStatus;
  // Persisted project policy and optimistic revision.
  // 持久项目策略与乐观修订。
  settings: VceProjectSettingsSummary;
  // Committed project metadata, independent from current indexing progress.
  // 已提交项目元数据，独立于当前索引进度。
  status: VceProjectStatusSummary;
  // Host-calculated current tool admission, never inferred from the latest job state.
  // 宿主计算的当前工具准入，绝不从最近作业状态推断。
  queryAvailable: boolean;
  // Last durable successful content update; null means no verifiable history.
  // 最近持久成功内容更新；空值表示没有可验证历史。
  lastSuccessfulUpdateUnixMs: number | null;
}

// VcePageRequest selects one bounded management page.
// VcePageRequest 选择一页有界管理数据。
export interface VcePageRequest {
  cursor?: string | null;
  limit: number;
}

// VcePageResponse returns stable rows and an optional continuation cursor.
// VcePageResponse 返回稳定记录与可选继续游标。
export interface VcePageResponse<T> {
  items: T[];
  nextCursor?: string | null;
  total?: number | null;
}

// VceChannelStateSummary reports one optional index channel.
// VceChannelStateSummary 报告一项可选索引通道。
export interface VceChannelStateSummary {
  state: string;
  reason?: string | null;
}

// VceFileSummary is one management-safe project file row.
// VceFileSummary 是一项管理安全的项目文件记录。
export interface VceFileSummary {
  path: string;
  blobName?: string | null;
  committedBlobName?: string | null;
  status: string;
  generation: number;
  size: number;
  chunkCount: number;
  vectorCount: number;
  symbolCount: number;
  pathChannel: VceChannelStateSummary;
  symbolChannel: VceChannelStateSummary;
  lastError?: string | null;
}

// VceChunkSummary is one bounded source chunk shown in Blob details.
// VceChunkSummary 是 Blob 详情显示的一段有界源码代码块。
export interface VceChunkSummary {
  chunkId: string;
  startLine: number;
  endLine: number;
  startCharOffset: number;
  kind: string;
  preview?: string | null;
  vectorReady: boolean;
}

// VceBlobDetailResponse describes one exact persisted Blob.
// VceBlobDetailResponse 描述一个精确持久化 Blob。
export interface VceBlobDetailResponse {
  blobName: string;
  path: string;
  generation: number;
  structureAnalyzerId?: string | null;
  structureFallbackReason?: string | null;
  chunks: VceChunkSummary[];
  pathChannel: VceChannelStateSummary;
  symbolChannel: VceChannelStateSummary;
}

// VceOperationSummary is one durable indexing operation row.
// VceOperationSummary 是一项持久索引操作记录。
export interface VceOperationSummary {
  operationId: string;
  kind: string;
  phase: string;
  generation: number;
  refreshMode?: string | null;
  refreshReason?: string | null;
  completedUnits: number;
  totalUnits: number;
  attempt: number;
  coalesced: boolean;
  error?: string | null;
}

// VceIntegrityIssueSummary is one actionable consistency diagnostic.
// VceIntegrityIssueSummary 是一项可操作的一致性诊断。
export interface VceIntegrityIssueSummary {
  issueId: string;
  kind: string;
  path?: string | null;
  blobName?: string | null;
  message: string;
}

// VceMaintenanceActionSummary is one explicit host-executable planned action.
// VceMaintenanceActionSummary 是一项明确可由宿主执行的计划动作。
export interface VceMaintenanceActionSummary {
  actionId: string;
  description: string;
  blobNames: string[];
  destructive: boolean;
}

// VceMaintenancePlanResponse is a read-only mutation preview.
// VceMaintenancePlanResponse 是一份只读修改预览。
export interface VceMaintenancePlanResponse {
  // Exact embedding identity frozen when the plan was previewed.
  // 预览计划时固定的精确向量身份。
  embeddingFingerprint: string;
  // Explicit target dimension presented for confirmation.
  // 提交确认时展示的明确目标维度。
  embeddingDimension: number;
  mode: string;
  actions: VceMaintenanceActionSummary[];
  affectedBlobCount: number;
}

// VceMaintenanceExecutionRequest copies one current plan into a guarded destructive command.
// VceMaintenanceExecutionRequest 将一份当前计划复制为受保护的破坏性命令。
export interface VceMaintenanceExecutionRequest {
  // Exact preview identity; execution must never substitute the latest selection.
  // 精确预览身份；执行绝不能替换为最新选择。
  expectedEmbeddingFingerprint: string;
  // Stable replay identity for this exact confirmed submission.
  // 此精确确认提交的稳定重放身份。
  idempotencyKey: string;
  // Exact workbench project identity.
  // 精确工作台项目身份。
  projectId: string;
  // Stable maintenance mode copied from the plan.
  // 从计划复制的稳定维护模式。
  mode: string;
  // Exact settings revision required by execution.
  // 执行要求的精确设置修订。
  expectedSettingsRevision: number;
  // Exact checkpoint visible when the plan was confirmed; null is valid only for an uninitialized rebuild.
  // 确认计划时可见的精确检查点；未初始化项目的重建可以使用空值。
  expectedCheckpointVersion: number | null;
  // Exact action identities copied from the plan.
  // 从计划复制的精确操作身份。
  actionIds: string[];
}

// Maintenance admits a durable indexing job or returns completed cleanup without scanning.
// 维护准入持久索引作业，或返回不扫描的已完成清理。
export type VceMaintenanceExecutionResponse =
  // Job is observed through the shared project progress lifecycle.
  // 作业通过共享项目进度生命周期观察。
  | { kind: "job"; job: VceJobSummary }
  // Cleanup returns its authoritative overview immediately after the bounded operation.
  // 清理在有界操作后立即返回权威概览。
  | { kind: "cleaned"; overview: VceProjectOverviewResponse };

// VceProjectChangedEvent invalidates one project lifecycle projection.
// VceProjectChangedEvent 使单个项目生命周期投影失效。
export interface VceProjectChangedEvent {
  projectId: string;
  settingsRevision: number;
  lifecycle: VceProjectLifecycleState;
  checkpointVersion?: number | null;
}
