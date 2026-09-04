export interface MemorySummary {
  // Stable memory identifier.
  // 稳定记忆标识。
  id: string;
  // Memory display title.
  // 记忆显示标题。
  title: string;
  // Memory category.
  // 记忆分类。
  category: string;
  // Memory state.
  // 记忆状态。
  state: string;
}

// VmmDeploymentMode identifies how Vulcan Code reaches the VMM runtime.
// VmmDeploymentMode 标识 Vulcan Code 连接 VMM 运行时的方式。
export type VmmDeploymentMode = "disabled" | "native" | "external";

// VmmSetupState identifies whether the user-facing setup is complete.
// VmmSetupState 标识面向用户的设置是否完整。
export type VmmSetupState =
  | "not_configured"
  | "checking"
  | "ready"
  | "incomplete"
  | "blocked";

// VmmLifecycleState identifies the selected VMM backend lifecycle.
// VmmLifecycleState 标识所选 VMM 后端生命周期。
export type VmmLifecycleState = "stopped" | "running" | "failed";

// VmmHealthState identifies the currently available memory experience.
// VmmHealthState 标识当前可用的记忆体验。
export type VmmHealthState =
  | "unknown"
  | "healthy"
  | "degraded"
  | "unavailable";

// VmmExperienceStateSummary combines independent VMM state dimensions.
// VmmExperienceStateSummary 汇总彼此独立的 VMM 状态维度。
export interface VmmExperienceStateSummary {
  // Configured deployment mode.
  // 已配置的部署方式。
  deployment: VmmDeploymentMode;
  // Setup readiness state.
  // 设置就绪状态。
  setup: VmmSetupState;
  // Runtime lifecycle state.
  // 运行时生命周期状态。
  lifecycle: VmmLifecycleState;
  // User-visible health state.
  // 用户可见健康状态。
  health: VmmHealthState;
}

// VmmPrerequisiteSummary describes one setup or runtime prerequisite.
// VmmPrerequisiteSummary 描述单个设置或运行前置条件。
export interface VmmPrerequisiteSummary {
  // Stable prerequisite identifier.
  // 稳定前置条件标识。
  id: string;
  // Stable prerequisite state.
  // 稳定前置条件状态。
  state: string;
  // Stable severity key.
  // 稳定严重程度键。
  severity: string;
  // Translation key for the short title.
  // 简短标题的翻译键。
  titleKey: string;
  // Translation key for the actionable description.
  // 可操作说明的翻译键。
  descriptionKey: string;
  // Controlled technical detail shown only in diagnostics.
  // 仅在诊断中显示的受控技术详情。
  technicalDetail?: string | null;
  // Stable recovery action kind.
  // 稳定恢复操作类型。
  actionKind?: string | null;
  // Optional route or section targeted by the recovery action.
  // 恢复操作指向的可选路由或分区。
  actionTarget?: string | null;
}

// VmmCapabilitySummary describes one independently available VMM capability.
// VmmCapabilitySummary 描述单项独立可用的 VMM 能力。
export interface VmmCapabilitySummary {
  // Stable capability identifier.
  // 稳定能力标识。
  id: string;
  // Whether the capability can currently be used.
  // 当前是否可以使用该能力。
  available: boolean;
  // Whether the user configuration enables the capability.
  // 用户配置是否启用该能力。
  enabled: boolean;
  // Stable capability state.
  // 稳定能力状态。
  state: string;
  // Optional user-facing reason key.
  // 可选的用户可见原因键。
  reason?: string | null;
  // Optional destination used to configure the capability.
  // 用于配置该能力的可选目标。
  configureTarget?: string | null;
}


// VmmUserDisplaySummary describes one durable VMM user without exposing a numeric id.
// VmmUserDisplaySummary 描述单个长期 VMM 用户且不暴露数字标识。
export interface VmmUserDisplaySummary {
  // Stable internal key used for follow-up requests.
  // 后续请求使用的稳定内部键。
  stableKey: string;
  // Human-readable user name.
  // 人类可读的用户名称。
  displayName: string;
  // Number of associated durable projects.
  // 关联的长期项目数量。
  linkedProjectCount?: number | null;
  // Compact memory state key.
  // 紧凑记忆状态键。
  memoryStatus: string;
  // Whether this user is protected from deletion.
  // 该用户是否受删除保护。
  protected: boolean;
}

// VmmProjectDisplaySummary describes one durable VMM project without exposing numeric ids.
// VmmProjectDisplaySummary 描述单个长期 VMM 项目且不暴露数字标识。
export interface VmmProjectDisplaySummary {
  // Stable internal key used for follow-up requests.
  // 后续请求使用的稳定内部键。
  stableKey: string;
  // Human-readable project name.
  // 人类可读的项目名称。
  displayName: string;
  // Human-readable canonical project path.
  // 人类可读的标准项目路径。
  displayPath: string;
  // Opaque VMM project reference used only for management requests.
  // 仅供管理请求使用的不透明 VMM 项目引用。
  actionPath: string;
  // Optional Vulcan Code project key bound to this VMM project.
  // 与该 VMM 项目绑定的可选 Vulcan Code 项目键。
  taskProjectKey?: string | null;
  // Whether this project has an explicit memory-rule override.
  // 该项目是否具有独立记忆规则覆盖。
  hasOverride?: boolean | null;
  // Compact project state key.
  // 紧凑项目状态键。
  state: string;
  // Whether this project is protected from deletion.
  // 该项目是否受删除保护。
  protected: boolean;
}

// VmmManagementCatalogSummary describes the cached VMM user and project directory.
// VmmManagementCatalogSummary 描述缓存的 VMM 用户与项目目录。
export interface VmmManagementCatalogSummary {
  // Whether the catalog is currently loading.
  // 目录当前是否正在加载。
  loading: boolean;
  // Durable user summaries.
  // 长期用户摘要。
  users: VmmUserDisplaySummary[];
  // Durable project summaries.
  // 长期项目摘要。
  projects: VmmProjectDisplaySummary[];
  // Last successful refresh timestamp.
  // 最近一次刷新成功时间。
  lastRefreshedAt?: string | null;
  // Controlled refresh error.
  // 受控的刷新错误。
  error?: string | null;
}

// VmmRecentActivitySummary describes recent memory activity without internal ids.
// VmmRecentActivitySummary 在不暴露内部标识的前提下描述近期记忆活动。
export interface VmmRecentActivitySummary {
  // Number of pending writeback items.
  // 待写回项目数量。
  pendingWritebacks: number;
  // Number of completed turns observed by profile synchronization.
  // 画像同步已观察的完成回合数量。
  completedTurns: number;
  // Latest durable profile update timestamp.
  // 最近一次长期画像更新时间。
  lastProfileUpdateAt?: string | null;
}

// VmmDisplayIdentitySummary provides user-facing names for the global default VMM identity.
// VmmDisplayIdentitySummary 提供全局默认 VMM 身份的用户可见名称。
export interface VmmDisplayIdentitySummary {
  // Default VMM project display name.
  // 默认 VMM 项目显示名称。
  projectName?: string | null;
  // Canonical path of the default VMM project.
  // 默认 VMM 项目的规范路径。
  projectPath?: string | null;
  // Default VMM user display name.
  // 默认 VMM 用户显示名称。
  userName?: string | null;
}

// VmmMemorySnapshot describes the full VMM memory management page.
// VmmMemorySnapshot 描述完整 VMM 记忆管理页面。
export interface VmmMemorySnapshot {
  // Product-facing VMM state dimensions.
  // 面向产品的 VMM 状态维度。
  experienceState: VmmExperienceStateSummary;
  // Structured setup and runtime prerequisites.
  // 结构化设置与运行前置条件。
  prerequisites: VmmPrerequisiteSummary[];
  // Independently projected memory capabilities.
  // 独立投影的记忆能力。
  capabilities: VmmCapabilitySummary[];
  // Cached VMM user and project directory.
  // 缓存的 VMM 用户与项目目录。
  managementCatalog: VmmManagementCatalogSummary;
  // Recent memory activity.
  // 近期记忆活动。
  recentActivity: VmmRecentActivitySummary;
  // User-facing global default VMM identity.
  // 用户可见的全局默认 VMM 身份。
  displayIdentity: VmmDisplayIdentitySummary;
  // Effective VMM configuration for the selected session.
  // 当前选中会话的最终 VMM 配置。
  effective: VmmEffectiveSummary;
  // Global editable VMM draft.
  // 全局可编辑 VMM 草稿。
  globalDraft: VmmConfigDraftSummary;
  // Selected project editable VMM draft.
  // 当前选中项目可编辑 VMM 草稿。
  projectDraft: VmmConfigDraftSummary;
  // Available project scopes.
  // 可用项目作用域。
  projectScopes: VmmProjectScopeSummary[];
  // Selected project id.
  // 当前选中项目标识。
  selectedProjectId?: string | null;
  // Selected conversation id.
  // 当前选中会话标识。
  selectedConversationId?: string | null;
  // Runtime side-effect summary.
  // 运行态副作用摘要。
  runtime: VmmRuntimeSummary;
  // Management sections.
  // 管理分区。
  sections: VmmManagementSectionSummary[];
  // Human-readable notice.
  // 人类可读提示。
  notice: string;
  // Transient native or external binding outcome attached only to one project-add response.
  // 仅附加到单次项目添加响应的原生或外置绑定结果。
  projectBindingNotice?: VmmProjectBindingNotice | null;
}

// VmmProjectBindingNotice reports the memory binding attempted after project creation.
// VmmProjectBindingNotice 报告项目创建后尝试的记忆绑定。
export interface VmmProjectBindingNotice {
  vulcanProjectId: string;
  status: "bound" | "failed";
  vmmProjectName?: string | null;
  nativeVmmProjectId?: number | null;
  externalVmmProjectId?: number | null;
  message: string;
}

// VmmEffectiveSummary describes resolved VMM settings.
// VmmEffectiveSummary 描述解析后的 VMM 设置。
export interface VmmEffectiveSummary {
  enabled: boolean;
  grpcTarget: string;
  grpcConfigured: boolean;
  userId?: number | null;
  projectId?: number | null;
  diagnostic: string;
  precheckDiagnostic: string;
  // Completed-turn writeback diagnostic key.
  // 完成回合写回诊断键。
  writebackDiagnostic: string;
  aiToolsEnabled: boolean;
  precheckEnabled: boolean;
  // Effective completed-turn writeback enablement.
  // 最终完成回合写回启用状态。
  memoryWritebackEnabled: boolean;
  implicitMemoryTurns: number;
  profileRefreshTurns: number;
  profileNodeLimit: number;
  injectionTag: string;
  injectionDescription: string;
}


// VmmConfigDraftSummary describes editable VMM config fields.
// VmmConfigDraftSummary 描述可编辑 VMM 配置字段。
export interface VmmConfigDraftSummary {
  hasOverride: boolean;
  // Global backend selector using native or external_grpc.
  // 使用 native 或 external_grpc 的全局后端选择器。
  backendMode: "native" | "external_grpc";
  // Native new-project policy using one fixed project or one project per VulcanCode tag.
  // 使用固定项目或按 VulcanCode 标签独立创建项目的原生新增项目策略。
  nativeProjectBindingMode: "fixed" | "per_project";
  // External new-project policy using one fixed project or one project per VulcanCode tag.
  // 使用固定项目或按 VulcanCode 标签独立创建项目的外置新增项目策略。
  externalProjectBindingMode: "fixed" | "per_project";
  // Stable native project id used by fixed binding.
  // 固定绑定使用的稳定原生项目标识。
  nativeFixedProjectId: string;
  // External automatic project Team/Space prefix.
  // 外置自动项目的 Team/Space 前缀。
  externalProjectPrefix: string;
  enabled: boolean;
  grpcTarget: string;
  grpcApiKey: string;
  managementBaseUrl: string;
  managementBearerToken: string;
  managementTimeoutMs: string;
  userId: string;
  projectId: string;
  receiveTimeoutMs: string;
  handshakeTimeoutMs: string;
  maxReceiveMessageBytes: string;
  grpcKeepaliveTimeMs: string;
  grpcKeepaliveTimeoutMs: string;
  grpcKeepalivePermitWithoutCalls: boolean;
  implicitMemoryTurns: string;
  profileRefreshTurns: string;
  profileNodeLimit: string;
  aiToolsEnabled: boolean;
  precheckEnabled: boolean;
  // Tri-state precheck policy mode for this editable layer.
  // 当前可编辑层的三态前置注入策略模式。
  precheckMode: string;
  // Editable completed-turn writeback boolean fallback.
  // 可编辑的完成回合写回布尔回退值。
  memoryWritebackEnabled: boolean;
  // Tri-state writeback policy mode for this editable layer.
  // 当前可编辑层的三态写回策略模式。
  memoryWritebackMode: string;
  injectionTag: string;
  injectionDescription: string;
}

// VmmProjectScopeSummary describes one project-level VMM scope.
// VmmProjectScopeSummary 描述单个项目级 VMM 作用域。
export interface VmmProjectScopeSummary {
  projectId: string;
  name: string;
  path: string;
  hasOverride: boolean;
  // External VMM project id explicitly persisted for this VulcanCode project.
  // 为该 VulcanCode 项目显式持久化的外置 VMM 项目标识。
  boundProjectId?: number | null;
}

// VmmRuntimeSummary describes selected-session VMM runtime side effects.
// VmmRuntimeSummary 描述当前选中会话的 VMM 运行态副作用。
export interface VmmRuntimeSummary {
  processReady: boolean;
  // Active native or external backend state.
  // 当前原生或外置后端状态。
  backendState: string;
  // Latest controlled backend error.
  // 最近一次受控后端错误。
  backendError?: string | null;
  scopeReady: boolean;
  scopeError?: string | null;
  precheckReady: boolean;
  precheckError?: string | null;
  postactionReady: boolean;
  postactionError?: string | null;
  profileInstructionReady: boolean;
  profileInstructionError?: string | null;
  rerankReady: boolean;
  rerankError?: string | null;
  outboxCount: number;
  profileSyncCount: number;
  completedTurns: number;
  updatedAt?: string | null;
  profilePreview: string;
}

// VmmManagementSectionSummary describes one VMM management section.
// VmmManagementSectionSummary 描述单个 VMM 管理分区。
export interface VmmManagementSectionSummary {
  id: string;
  key: string;
  label: string;
  description: string;
}

// VmmMemoryActionRequest describes one VMM action request.
// VmmMemoryActionRequest 描述单个 VMM 动作请求。
export interface VmmMemoryActionRequest {
  // Stable action id.
  // 稳定动作标识。
  action: string;
  // Stable UUID identifying this exact user operation across transport retries.
  // 跨传输重试标识本次精确用户操作的稳定 UUID。
  commandId: string;
  // VMM configuration scope, either global or project.
  // VMM 配置作用域，可为 global 或 project。
  scope: string;
  // Optional task project id used by project scope.
  // 项目作用域使用的可选任务项目标识。
  projectId?: string | null;
  // Exact persisted conversation id required by session-scoped actions.
  // 会话级动作必须提供的准确已持久化会话标识。
  conversationId?: string | null;
  // Action-specific JSON payload.
  // 动作专用 JSON 载荷。
  payload?: unknown;
}

// VmmNativeProjectSummary describes one one-level native VMM project.
// VmmNativeProjectSummary 描述一个一级原生 VMM 项目。
export interface VmmNativeProjectSummary {
  projectId: number;
  name: string;
  tag: string;
  protected: boolean;
  ownerVulcanProjectIds: string[];
  linkedVulcanProjectIds: string[];
}

// VmmNativeProjectBindingSummary describes one stable write owner plus recall-only links.
// VmmNativeProjectBindingSummary 描述一个稳定写入所有者及仅召回连带项目。
export interface VmmNativeProjectBindingSummary {
  vulcanProjectId: string;
  ownerProjectId: number;
  linkedProjectIds: number[];
}

// VmmNativeProjectCatalogSummary contains every native project and current binding.
// VmmNativeProjectCatalogSummary 包含全部原生项目与当前绑定。
export interface VmmNativeProjectCatalogSummary {
  projects: VmmNativeProjectSummary[];
  bindings: VmmNativeProjectBindingSummary[];
}

// VmmNativeProjectContentCounts summarizes project-owned native Core records.
// VmmNativeProjectContentCounts 汇总项目归属的原生 Core 记录。
export interface VmmNativeProjectContentCounts {
  sessions: number;
  turns: number;
  memories: number;
  profileNodes: number;
  contextEvidence: number;
  compactBoundaries: number;
  vectorRows: number;
}

// VmmNativeMemoryLevelCounts counts memories across the four authoritative Core levels.
// VmmNativeMemoryLevelCounts 按 Core 的四个权威等级统计记忆。
export interface VmmNativeMemoryLevelCounts {
  session: number;
  phase: number;
  stable: number;
  persistent: number;
}

// VmmNativeTurnStatusCounts counts turns across the authoritative processing states.
// VmmNativeTurnStatusCounts 按权威处理状态统计回合。
export interface VmmNativeTurnStatusCounts {
  pending: number;
  completed: number;
  processedUnknown: number;
  analysisBypassed: number;
  corrupted: number;
}

// VmmNativeMemorySessionSummary describes one project-owned host session.
// VmmNativeMemorySessionSummary 描述一个由项目拥有的宿主会话。
export interface VmmNativeMemorySessionSummary {
  sessionSourceId: string;
  displayTitle?: string | null;
  turnCount: number;
  turnStatuses: VmmNativeTurnStatusCounts;
  memoryCount: number;
  directMemoryCount: number;
  memoryLevels: VmmNativeMemoryLevelCounts;
  firstActivityAtUnixMs?: number | null;
  lastActivityAtUnixMs?: number | null;
}

// VmmNativeTimelineItemSummary describes one complete persisted timeline item.
// VmmNativeTimelineItemSummary 描述一条完整持久化时间线项目。
export interface VmmNativeTimelineItemSummary {
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  occurredAtUnixMs: number;
}

// VmmNativeTurnRecordSummary describes one canonical turn and its extraction outcome.
// VmmNativeTurnRecordSummary 描述一个规范回合及其提取结果。
export interface VmmNativeTurnRecordSummary {
  turnId: number;
  sessionSourceId: string;
  createdAtUnixMs: number;
  status: "pending" | "completed" | "processed_unknown" | "analysis_bypassed" | "corrupted";
  extracted?: boolean | null;
  userPreview: string;
  assistantPreview: string;
  timeline: VmmNativeTimelineItemSummary[];
  analysisDetails?: string | null;
  generatedMemoryIds: number[];
  memoryLevels: VmmNativeMemoryLevelCounts;
  analysisJobState?: string | null;
  analysisAttemptCount: number;
  analysisLastError?: string | null;
}

// VmmNativeMemoryRecordSummary describes one durable memory with exact provenance.
// VmmNativeMemoryRecordSummary 描述一条具有准确来源的持久记忆。
export interface VmmNativeMemoryRecordSummary {
  memoryId: number;
  sourceTurnId?: number | null;
  sessionSourceId?: string | null;
  summary: string;
  content: string;
  category: "general" | "architecture_decision" | "tech_spec_api" | "business_logic"
    | "requirement_todo" | "project_context" | "logical_bug_debt" | "security_policy";
  sourceKind: "turn" | "direct" | "import" | "system_seed" | "legacy";
  state: "active" | "superseded" | "deleted" | "expired" | "recycled";
  memoryLevel: "session" | "phase" | "stable" | "persistent";
  priority: number;
  tags: string[];
  createdAtUnixMs: number;
  updatedAtUnixMs: number;
  expiresAtUnixMs?: number | null;
  statusReason?: string | null;
}

// VmmNativeMemoryRecordsSummary is one coherent project/session provenance response.
// VmmNativeMemoryRecordsSummary 是一个一致的项目/会话来源响应。
export interface VmmNativeMemoryRecordsSummary {
  storageVersion: number;
  projectId: number;
  sessions: VmmNativeMemorySessionSummary[];
  selectedSessionSourceId?: string | null;
  turns: VmmNativeTurnRecordSummary[];
  memories: VmmNativeMemoryRecordSummary[];
}

// VmmNativeSessionRename describes one deterministic migration collision rename.
// VmmNativeSessionRename 描述一次确定性的迁移冲突会话改名。
export interface VmmNativeSessionRename {
  from: string;
  to: string;
}

// VmmNativeMigrationRecordSummary describes one primary record shown before migration.
// VmmNativeMigrationRecordSummary 描述迁移前展示的一条主记录。
export interface VmmNativeMigrationRecordSummary {
  recordKind: "session" | "turn" | "memory" | "project_profile";
  recordId: string;
  sessionSourceId: string | null;
  summary: string | null;
}

// VmmNativeMigrationPreview is the side-effect-free native migration impact report.
// VmmNativeMigrationPreview 是无副作用的原生迁移影响报告。
export interface VmmNativeMigrationPreview {
  storageVersion: number;
  sourceProjectId: number;
  targetProjectId: number;
  source: VmmNativeProjectContentCounts;
  targetBefore: VmmNativeProjectContentCounts;
  targetAfter: VmmNativeProjectContentCounts;
  sessionRenames: VmmNativeSessionRename[];
  records: VmmNativeMigrationRecordSummary[];
  previewFingerprint: string;
}

// VmmNativeMigrationResult is the committed native migration result.
// VmmNativeMigrationResult 是已提交的原生迁移结果。
export interface VmmNativeMigrationResult {
  storageVersion: number;
  moved: VmmNativeProjectContentCounts;
  sessionRenames: VmmNativeSessionRename[];
}

// VmmNativeClearPreview is the side-effect-free native clear impact report.
// VmmNativeClearPreview 是无副作用的原生清空影响报告。
export interface VmmNativeClearPreview {
  storageVersion: number;
  projectId: number;
  removed: VmmNativeProjectContentCounts;
}

// VmmNativeClearResult is the committed native clear result.
// VmmNativeClearResult 是已提交的原生清空结果。
export interface VmmNativeClearResult {
  storageVersion: number;
  removed: VmmNativeProjectContentCounts;
}

// VmmNativeProjectDeletePreview reports consumers and content affected by deletion.
// VmmNativeProjectDeletePreview 报告删除影响的使用方与内容。
export interface VmmNativeProjectDeletePreview {
  storageVersion: number;
  project: VmmNativeProjectSummary;
  content: VmmNativeProjectContentCounts;
}

// VmmProtocolUserEntry is one exact durable VMM user row.
// VmmProtocolUserEntry 是一条准确的长期 VMM 用户记录。
export interface VmmProtocolUserEntry {
  user_id: number;
  user_name: string;
}

// VmmProtocolProjectEntry is one exact durable VMM project row.
// VmmProtocolProjectEntry 是一条准确的长期 VMM 项目记录。
export interface VmmProtocolProjectEntry {
  project_id: number;
  project_name: string;
  display_path: string;
}

// VmmResolveUserActionValue is the exact user resolution contract.
// VmmResolveUserActionValue 是准确的用户解析契约。
export interface VmmResolveUserActionValue {
  user?: VmmProtocolUserEntry;
  message: string;
  created: boolean;
  exists: boolean;
  trace_id: string;
}

// VmmResolveProjectActionValue is the exact project resolution contract.
// VmmResolveProjectActionValue 是准确的项目解析契约。
export interface VmmResolveProjectActionValue {
  project?: VmmProtocolProjectEntry;
  message: string;
  trace_id: string;
}

// VmmEnsureProjectActionValue is the exact guarded project creation contract.
// VmmEnsureProjectActionValue 是准确的受保护项目创建契约。
export interface VmmEnsureProjectActionValue {
  project?: VmmProtocolProjectEntry;
  message: string;
  exists: boolean;
  needs_confirm: boolean;
  created_project: boolean;
  trace_id: string;
}

// VmmDeleteUserActionValue is the exact guarded user deletion contract.
// VmmDeleteUserActionValue 是准确的受保护用户删除契约。
export interface VmmDeleteUserActionValue {
  user?: VmmProtocolUserEntry;
  message: string;
  requires_confirmation: boolean;
  confirmation_code: string;
  deleted_users: number;
  trace_id: string;
}

// VmmDeleteProjectActionValue is the exact guarded project deletion contract.
// VmmDeleteProjectActionValue 是准确的受保护项目删除契约。
export interface VmmDeleteProjectActionValue {
  project?: VmmProtocolProjectEntry;
  message: string;
  needs_confirm: boolean;
  deleted_projects: number;
  trace_id: string;
}

// VmmMigrateProjectActionValue is the exact guarded project migration contract.
// VmmMigrateProjectActionValue 是准确的受保护项目迁移契约。
export interface VmmMigrateProjectActionValue {
  source_project?: VmmProtocolProjectEntry;
  target_project?: VmmProtocolProjectEntry;
  message: string;
  needs_confirm: boolean;
  migrated_sessions: number;
  migrated_memories: number;
  rebuilt_vector_rows: number;
  trace_id: string;
}

// VmmProfileNodeActionValue is one exact profile node projection.
// VmmProfileNodeActionValue 是一个准确的画像节点投影。
export interface VmmProfileNodeActionValue {
  content: string;
  priority: string;
  level: string;
  profile_date: string;
}

// VmmProfileNodesActionValue is the exact profile-node list contract.
// VmmProfileNodesActionValue 是准确的画像节点列表契约。
export interface VmmProfileNodesActionValue {
  nodes: VmmProfileNodeActionValue[];
  trace_id: string;
}

// VmmProfileBundleActionValue is the exact final profile bundle contract.
// VmmProfileBundleActionValue 是准确的最终画像组合契约。
export interface VmmProfileBundleActionValue {
  combined_text: string;
  // Project profile section returned independently from the combined prompt text.
  // 与组合提示词文本独立返回的项目画像段落。
  project_profile: string;
  // Fixed or selected user profile section returned independently from the combined prompt text.
  // 与组合提示词文本独立返回的固定或所选用户画像段落。
  user_profile: string;
  trace_id: string;
}

// VmmProfileInstructionActionValue is the exact profile instruction result.
// VmmProfileInstructionActionValue 是准确的画像指令结果。
export interface VmmProfileInstructionActionValue {
  accepted_nodes: VmmProfileNodeActionValue[];
  retired_nodes: unknown[];
  review_reason: string;
  trace_id: string;
}

// VmmMemorySearchHitActionValue is one exact memory-search hit.
// VmmMemorySearchHitActionValue 是一条准确的记忆检索命中。
export interface VmmMemorySearchHitActionValue {
  memory_id: number;
  source_turn_id: number;
  abstract_text: string;
  details_preview: string;
  category: string;
  created_datetime: string;
}

// VmmSearchMemoryEventsActionValue is the exact grouped memory search contract.
// VmmSearchMemoryEventsActionValue 是准确的分组记忆检索契约。
export interface VmmSearchMemoryEventsActionValue {
  results: Array<{
    query_index: number;
    query: string;
    hits: VmmMemorySearchHitActionValue[];
  }>;
  trace_id: string;
}

// VmmTurnDetailActionValue is one exact source-turn detail.
// VmmTurnDetailActionValue 是一条准确的来源回合详情。
export interface VmmTurnDetailActionValue {
  turn_id: number;
  user_question: string;
  assistant_answer: string;
  detail: string;
}

// VmmGetTurnDetailsActionValue is the exact source-turn detail contract.
// VmmGetTurnDetailsActionValue 是准确的来源回合详情契约。
export interface VmmGetTurnDetailsActionValue {
  turns: VmmTurnDetailActionValue[];
  trace_id: string;
}

// VmmNativeResourceSettingsSummary is the persisted native Core composition selection.
// VmmNativeResourceSettingsSummary 是持久化的原生 Core 组合选择。
export interface VmmNativeResourceSettingsSummary {
  promptBundle: string;
  piiLanguage: string;
  noiseLanguage: string;
  noiseEnabled: boolean;
  noiseSemanticEnabled: boolean;
  noiseSemanticThreshold: number;
}

// VmmNativeResourceFileSummary describes one independently selectable embedded resource.
// VmmNativeResourceFileSummary 描述一份可独立选择的内置资源。
export interface VmmNativeResourceFileSummary {
  path: string;
  group: "prompt" | "pii" | "noise";
  selector: string;
  fileName: string;
  highRisk: boolean;
  hasUserOverride: boolean;
}

// VmmNativeResourceCatalogSummary exposes resource selectors and exact user override state.
// VmmNativeResourceCatalogSummary 暴露资源选择项与准确用户覆盖状态。
export interface VmmNativeResourceCatalogSummary {
  overrideRoot: string;
  settings: VmmNativeResourceSettingsSummary;
  promptBundles: string[];
  promptScenes: string[];
  piiLanguages: string[];
  noiseLanguages: string[];
  files: VmmNativeResourceFileSummary[];
}

// VmmNativeResourceFileContent carries immutable, effective, and optional user text.
// VmmNativeResourceFileContent 携带不可变、生效与可选用户文本。
export interface VmmNativeResourceFileContent {
  path: string;
  builtInText: string;
  effectiveText: string;
  userOverrideText?: string | null;
  hasUserOverride: boolean;
  highRisk: boolean;
}

// VmmNativePiiTestResult reports common-plus-locale redaction behavior.
// VmmNativePiiTestResult 报告 common 加区域脱敏行为。
export interface VmmNativePiiTestResult {
  outputText: string;
  redactions: Array<{ ruleName: string; matchCount: number }>;
}

// VmmNativePiiFormulaMatchSummary describes one regex candidate and condition-formula decision.
// VmmNativePiiFormulaMatchSummary 描述一个正则候选及条件公式判定。
export interface VmmNativePiiFormulaMatchSummary {
  startByte: number;
  endByte: number;
  matchedText: string;
  captures: Array<string | null>;
  conditionAccepted: boolean;
}

// VmmNativePiiFormulaTestResult reports an ad-hoc rule result and its complete local trace.
// VmmNativePiiFormulaTestResult 报告临时规则结果及其完整本地轨迹。
export interface VmmNativePiiFormulaTestResult {
  outputText: string;
  matches: VmmNativePiiFormulaMatchSummary[];
}

// VmmNativeNoiseTestResult reports deterministic common-plus-locale regex behavior.
// VmmNativeNoiseTestResult 报告确定性 common 加区域正则行为。
export interface VmmNativeNoiseTestResult {
  isNoise: boolean;
  category?: string | null;
  confidence: number;
  mode: "deterministic_regex";
}

// VmmMemoryActionResponse is the tagged response union consumed by product pages.
// VmmMemoryActionResponse 是产品页面消费的带标签响应联合。
export type VmmMemoryActionResponse =
  | { kind: "empty" }
  | { kind: "context_injection"; value: Record<string, unknown> }
  | { kind: "healthz"; value: { status: string } }
  | {
      kind: "users_projects";
      value: { users: VmmProtocolUserEntry[]; projects: VmmProtocolProjectEntry[] };
    }
  | {
      kind: "resolved";
      value: { user: VmmResolveUserActionValue; project: VmmResolveProjectActionValue };
    }
  | { kind: "ensure_project"; value: VmmEnsureProjectActionValue }
  | { kind: "delete_user"; value: VmmDeleteUserActionValue }
  | { kind: "delete_project"; value: VmmDeleteProjectActionValue }
  | { kind: "migrate_project"; value: VmmMigrateProjectActionValue }
  | { kind: "profile_nodes"; value: VmmProfileNodesActionValue }
  | { kind: "profile_bundle"; value: VmmProfileBundleActionValue }
  | { kind: "profile_instruction"; value: VmmProfileInstructionActionValue }
  | { kind: "resolve_user"; value: VmmResolveUserActionValue }
  | { kind: "search_memory_events"; value: VmmSearchMemoryEventsActionValue }
  | { kind: "get_turn_details"; value: VmmGetTurnDetailsActionValue }
  | { kind: "write_memories"; value: { items: unknown[]; trace_id: string } }
  | {
      kind: "delete_memories";
      value: {
        deleted_memory_ids: number[];
        not_found_memory_ids: number[];
        deleted_vector_rows: number;
        trace_id: string;
      };
    }
  | { kind: "native_project_catalog"; value: VmmNativeProjectCatalogSummary }
  | { kind: "native_memory_records"; value: VmmNativeMemoryRecordsSummary }
  | { kind: "native_project_delete_preview"; value: VmmNativeProjectDeletePreview }
  | { kind: "native_migration_preview"; value: VmmNativeMigrationPreview }
  | { kind: "native_migration"; value: VmmNativeMigrationResult }
  | { kind: "native_clear_preview"; value: VmmNativeClearPreview }
  | { kind: "native_clear"; value: VmmNativeClearResult }
  | { kind: "native_resource_catalog"; value: VmmNativeResourceCatalogSummary }
  | { kind: "native_resource_file"; value: VmmNativeResourceFileContent }
  | { kind: "native_pii_test"; value: VmmNativePiiTestResult }
  | { kind: "native_pii_formula_test"; value: VmmNativePiiFormulaTestResult }
  | { kind: "native_noise_test"; value: VmmNativeNoiseTestResult }
  | { kind: "management"; value: unknown };

// VmmMemoryActionResult describes one VMM action response.
// VmmMemoryActionResult 描述单个 VMM 动作响应。
export interface VmmMemoryActionResult {
  action: string;
  summary: string;
  kind: string;
  request: unknown;
  response: unknown;
  data: VmmMemoryActionResponse;
}

// VmmConfigSaveRequest describes one VMM config save request.
// VmmConfigSaveRequest 描述单个 VMM 配置保存请求。
export interface VmmConfigSaveRequest {
  scope: string;
  projectId?: string | null;
  draft: VmmConfigDraftSummary;
  // Explicit decisions for every currently unbound VulcanCode project.
  // 每个当前未绑定 VulcanCode 项目的明确决定。
  bindingPlan?: VmmProjectBindingPlanEntry[];
}

// VmmProjectBindingPlanEntry is one explicit enable-time binding decision for the selected backend.
// VmmProjectBindingPlanEntry 是启用时针对所选后端的一项明确绑定决定。
export interface VmmProjectBindingPlanEntry {
  vulcanProjectId: string;
  action: "create_and_bind" | "bind_existing";
  projectName?: string | null;
  targetProjectId?: number | null;
}
