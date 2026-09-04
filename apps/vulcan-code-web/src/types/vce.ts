// VceProjectLifecycleState identifies one host-owned code-semantic lifecycle.
// VceProjectLifecycleState 标识一项由宿主持有的代码语义生命周期。
export type VceProjectLifecycleState =
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

// VceProjectSettingsSummary contains persisted settings plus host-owned metadata.
// VceProjectSettingsSummary 包含持久化设置与宿主持有的元数据。
export interface VceProjectSettingsSummary {
  enabled: boolean;
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
  settings: VceProjectSettingsSummary;
  status: VceProjectStatusSummary;
  ai?: VceAiCapabilitySummary | null;
  counts?: VceIndexCountsSummary | null;
  integrity?: VceIndexIntegritySummary | null;
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
  mode: string;
  actions: VceMaintenanceActionSummary[];
  affectedBlobCount: number;
}

// VceMaintenanceExecutionRequest copies one current plan into a guarded destructive command.
// VceMaintenanceExecutionRequest 将一份当前计划复制为受保护的破坏性命令。
export interface VceMaintenanceExecutionRequest {
  // Exact workbench project identity.
  // 精确工作台项目身份。
  projectId: string;
  // Stable maintenance mode copied from the plan.
  // 从计划复制的稳定维护模式。
  mode: string;
  // Exact settings revision required by execution.
  // 执行要求的精确设置修订。
  expectedSettingsRevision: number;
  // Exact checkpoint visible when the plan was confirmed.
  // 确认计划时可见的精确检查点。
  expectedCheckpointVersion: number;
  // Exact action identities copied from the plan.
  // 从计划复制的精确操作身份。
  actionIds: string[];
}

// VceOperationMutationRequest targets one durable or active operation under revision guards.
// VceOperationMutationRequest 在修订保护下指向一项持久或活动操作。
export interface VceOperationMutationRequest {
  // Exact workbench project identity.
  // 精确工作台项目身份。
  projectId: string;
  // Stable visible operation identity.
  // 稳定可见操作身份。
  operationId: string;
  // Exact settings revision required by mutation.
  // 修改要求的精确设置修订。
  expectedSettingsRevision: number;
  // Optional checkpoint visible when the action was requested.
  // 请求操作时可见的可选检查点。
  expectedCheckpointVersion?: number | null;
}

// VceProjectChangedEvent invalidates one project lifecycle projection.
// VceProjectChangedEvent 使单个项目生命周期投影失效。
export interface VceProjectChangedEvent {
  projectId: string;
  settingsRevision: number;
  lifecycle: VceProjectLifecycleState;
  checkpointVersion?: number | null;
}
