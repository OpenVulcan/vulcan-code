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
  // Resolved enablement for a new conversation in this exact host project.
  // 当前准确宿主项目中新会话的已解析启用状态。
  memoryEnabled: boolean;
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
  // Null identifies content-only sessions, never an inferred active state.
  // null 标识只有内容的会话，绝不推断为活动状态。
  status: "active" | "archived" | "recycled" | "forgotten" | null;
  sessionSourceId: string;
  displayTitle?: string | null;
  turnCount: number;
  turnStatuses: VmmNativeTurnStatusCounts;
  memoryCount: number;
  profileNodeCount: number;
  directMemoryCount: number;
  memoryLevels: VmmNativeMemoryLevelCounts;
  firstActivityAtUnixMs?: number | null;
  lastActivityAtUnixMs?: number | null;
}

// NativeSessionListStatus explicitly distinguishes all rows from unregistered content-only rows.
// NativeSessionListStatus 明确区分全部记录与未登记的仅内容记录。
export type VmmNativeSessionListStatus = "all" | "active" | "archived" | "recycled" | "forgotten" | "unregistered";

// VmmNativeSessionListQuery carries server-side filters and a bounded page, never identity-by-title bindings.
// VmmNativeSessionListQuery 携带服务端筛选及有界分页，不按标题绑定身份。
export interface VmmNativeSessionListQuery {
  // Exact owner and explicit state/text filters.
  // 准确归属及明确状态/文本筛选。
  vmmProjectId: number;
  status: VmmNativeSessionListStatus;
  // Exact persisted turn-state filter and deterministic page ordering.
  // 准确持久回合状态筛选及确定性分页排序。
  extractionStatus: "all" | "pending" | "completed" | "processed_unknown" | "analysis_bypassed" | "corrupted";
  sort: "last_activity_desc" | "last_activity_asc" | "title_asc";
  search: string;
  // Inclusive lower and exclusive upper last-activity bounds in Unix milliseconds.
  // 最近活动 Unix 毫秒边界，下界包含、上界不包含。
  lastActivityFromUnixMs: number | null;
  lastActivityBeforeUnixMs: number | null;
  // Filtered-row offset and page size from 1 through 100.
  // 已筛选记录偏移及 1 至 100 的分页大小。
  offset: number;
  limit: number;
  // Core content version from the preceding page; null explicitly starts a fresh read.
  // 前页 Core 内容版本；null 明确开始新的读取。
  expectedStorageVersion: number | null;
}

// VmmNativeSessionListPage contains only this page's lightweight summaries and its full filtered count.
// VmmNativeSessionListPage 仅包含本页轻量摘要及完整筛选计数。
export interface VmmNativeSessionListPage {
  // Exact owner and coherent content version.
  // 准确归属及一致内容版本。
  projectId: number;
  storageVersion: number;
  // Full filtered count, accepted offset and accepted bounded page size.
  // 完整筛选数量、已接纳偏移及已接纳有界分页大小。
  total: number;
  offset: number;
  limit: number;
  // Only this page's ordered summaries, without turn or memory bodies.
  // 仅本页有序摘要，不含回合或记忆正文。
  sessions: VmmNativeMemorySessionSummary[];
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
  // Ordered preceding IDs resolved by the native server within the same owner session.
  // 原生服务端在同归属会话中解析的有序前序 ID。
  previousTurnIds: number[];
  // Ordered following IDs resolved by the native server within the same owner session.
  // 原生服务端在同归属会话中解析的有序后续 ID。
  nextTurnIds: number[];
  sessionSourceId: string;
  createdAtUnixMs: number;
  status: "pending" | "completed" | "processed_unknown" | "analysis_bypassed" | "corrupted";
  extracted?: boolean | null;
  userPreview: string;
  assistantPreview: string;
  timeline: VmmNativeTimelineItemSummary[];
  analysisDetails?: string | null;
  generatedMemoryIds: number[];
  generatedProfileNodeIds: number[];
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
  profileNodes: VmmNativeProfileRecordSummary[];
}

// VmmNativeProfileStateCounts counts every effective native profile lifecycle state.
// VmmNativeProfileStateCounts 统计全部有效原生画像生命周期状态。
export interface VmmNativeProfileStateCounts {
  active: number;
  inactive: number;
  pending: number;
  expired: number;
  scheduled: number;
}

// VmmNativeProfileRecordSummary describes one all-state native profile audit row.
// VmmNativeProfileRecordSummary 描述一条全状态原生画像审计记录。
export interface VmmNativeProfileRecordSummary {
  profileNodeId: number;
  target: "user" | "project";
  storedState: "active" | "inactive" | "pending" | "expired";
  effectiveState: "active" | "inactive" | "pending" | "expired" | "scheduled";
  activeForInjection: boolean;
  content: string;
  priority: number;
  level: "transient" | "situational" | "stable" | "persistent";
  sourceKind: "turn_extract" | "manual_instruction" | "system_seed"
    | "retained_after_subject_delete";
  sourceId?: string | null;
  sourceTurnIds: number[];
  effectiveDateAtUnixMs?: number | null;
  expiresAtUnixMs?: number | null;
  statusReason?: string | null;
  updatedAtUnixMs: number;
}

// VmmNativeProfileRecordsSummary contains all profile states for one native target.
// VmmNativeProfileRecordsSummary 包含一个原生目标的全部画像状态。
export interface VmmNativeProfileRecordsSummary {
  storageVersion: number;
  projectId: number;
  target: "user" | "project";
  counts: VmmNativeProfileStateCounts;
  nodes: VmmNativeProfileRecordSummary[];
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
  // The existing Rust protocol serializes this enum; it is the sole authority for content selection.
  // 既有 Rust 协议会序列化此枚举；它是选择正文表示的唯一权威。
  mode: "PROFILE_BUNDLE_MODE_UNSPECIFIED" | "PROFILE_BUNDLE_MODE_FULL" | "PROFILE_BUNDLE_MODE_SPLIT";
  combined_text: string;
  // Project section is consumable only in Split mode; Full deliberately leaves it empty.
  // 项目分区仅在 Split 模式可消费；Full 有意将其留空。
  project_profile: string;
  // User section is consumable only in Split mode; it must not be appended to Full text.
  // 用户分区仅在 Split 模式可消费；不能追加到 Full 正文中。
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
export interface VmmMemorySearchRetrieval {
  // Creation and retrieval sources remain independent, and missing server facts are never synthesized.
  // 创建及检索来源保持独立，不补造缺失的服务端事实。
  source_kind: string;
  origin: string;
  origin_label: string;
  origin_explanation: string;
  final_score: number;
  // Counts are meaningful only when the context stage was observed, not merely configured.
  // 计数仅在已观察情境阶段时有意义，不仅依据配置。
  context_observed: boolean;
  support_count: number;
  rebuttal_count: number;
  matched_context_values: string[];
  matched_context_support_count: number;
  matched_context_rebuttal_count: number;
  matched_context_score_delta: number | null;
}

// One search hit can omit retrieval facts when the external protocol does not supply them.
// 外置协议未提供检索事实时，单条命中可不含该信息。
export interface VmmMemorySearchHitActionValue {
  memory_id: number;
  source_turn_id: number;
  abstract_text: string;
  details_preview: string;
  category: string;
  created_datetime: string;
  retrieval?: VmmMemorySearchRetrieval;
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

// VmmNativeRecallPolicySummary contains only editable values with exact Core consumers.
// VmmNativeRecallPolicySummary 仅包含具有准确 Core 消费者的可编辑值。
export interface VmmNativeRecallPolicySummary {
  // Same-session history window for PreCheck, independent of extraction.
  // 独立于提炼的 PreCheck 同会话历史窗口。
  precheckHistoryTurnLimit: number;
  // Approximate historical-input token budget for PreCheck.
  // PreCheck 历史输入的近似 Token 预算。
  precheckMaxInputTokens: number;
  // Per-query recall window.
  // 每个查询的召回窗口。
  precheckRecallTopK: number;
  // Merged and redacted L2 candidate limit.
  // 跨组合并脱敏后的 L2 候选上限。
  precheckReviewCandidateLimit: number;
  // Query-local reviewer threshold, not a raw vector/BM25 score.
  // 查询内审核阈值，不是原始向量/BM25 分值。
  precheckMinReviewScore: number;
  // Query fan-out bound used by retrieval.
  // 检索使用的查询扇出上限。
  maxSearchQueries: number;
  // Already-refined historical turns for PostAction L1.
  // PostAction L1 的已提炼历史轮数。
  postactionHistoryTurnLimit: number;
  // Approximate target-plus-history token budget for PostAction L1.
  // PostAction L1 目标轮次与历史的近似 Token 预算。
  postactionMaxInputTokens: number;
  // Defaults for newly extracted memories only; existing records and fixed-user profiles are unchanged.
  // 仅用于新提炼记忆的默认值；既有记录及固定用户画像不变。
  postactionMemoryPriority: number;
  postactionMemoryStability: "working" | "stable" | "archival";
  postactionMemoryLevel: "session" | "phase" | "stable" | "persistent";
  postactionMemoryRefreshWeight: number;
  // Lifetime is persisted in exact milliseconds and starts at the original turn timestamp.
  // 有效期以准确毫秒持久化，从原始回合时间起算。
  postactionMemoryExpiryMs: number;
}

// VmmNativeRetrievalPolicySummary exposes business ranking values without producer identities.
// VmmNativeRetrievalPolicySummary 暴露业务排序值，不暴露生产者身份。
export interface VmmNativeRetrievalPolicySummary {
  // Enables fusion and sets its shared physical window floor.
  // 启用融合并设置共同物理窗口下限。
  hybridEnabled: boolean;
  lexicalCandidateFloor: number;
  // Exact signed-64 positive constant; never convert this field to a JavaScript number.
  // 准确的有符号64位正常数；绝不转换为 JavaScript 数值。
  rrfK: string;
  // Reviewer score floor for replacement, separately from exact cosine hard deduplication.
  // 替代审核评分下限，与准确余弦硬去重独立。
  replaceMinSimilarity: number;
  hardDedupeCosineThreshold: number;
  // Hard-dedupe, replacement-review, and optional rerank windows.
  // 硬去重、替代审核及可选重排窗口。
  hardDedupePoolTopK: number;
  replacementTopK: number;
  rerankTopN: number;
  // MMR enablement and relevance weight.
  // MMR 开关及相关性权重。
  mmrEnabled: boolean;
  mmrLambda: number;
  // Read-time decay enablement and its distribution parameters.
  // 读取时衰减开关及其分布参数。
  weibullEnabled: boolean;
  weibullShape: number;
  weibullScaleHours: number;
  weibullMinMultiplier: number;
  // Nonnegative reinforcement and cross-session adoption contributions.
  // 非负强化及跨会话采纳贡献。
  weibullReinforceWeight: number;
  weibullCrossSessionBoost: number;
}

// Project retention owns only data rules; worker cadence and personal profiles remain global.
// 项目保留仅持有数据规则；工作器周期和个人画像保持全局。
export type VmmNativeProjectRetentionRules = Pick<VmmNativeRetentionSettingsSummary,
  "turnHotWindowSize" | "sessionIdleMs" | "trashRetentionMs" | "protectedPriorityFloor" |
  "protectedMemoryLevelFloor" | "skipProtectedSharedMemories">

// Summary carries one exact memory-owner identity and explicit whole-group inheritance.
// 摘要携带一个准确记忆归属方身份及显式完整配置组继承信息。
export interface VmmNativeProjectRetentionSummary {
  projectId: number;
  overrides: VmmNativeProjectRetentionRules | null;
  effective: VmmNativeProjectRetentionRules;
  global: VmmNativeProjectRetentionRules;
}

// VmmNativeRetentionSettingsSummary owns editable cleanup rules, not internal work limits.
// VmmNativeRetentionSettingsSummary 持有可编辑清洗规则，不包含内部工作上限。
export interface VmmNativeRetentionSettingsSummary {
  // Periodic switch and independently configured whole-unit scan periods.
  // 周期开关及独立配置的整单位扫描周期。
  enabled: boolean;
  scanIntervalMinutes: number;
  profileExpiryIntervalSeconds: number;
  // Per-session hot window and exact millisecond retention durations.
  // 每会话热窗口及准确的毫秒保留时长。
  turnHotWindowSize: number;
  sessionIdleMs: number;
  trashRetentionMs: number;
  // Shared-memory protection thresholds use the Core priority and level scales.
  // 共享记忆保护阈值使用 Core 优先级与等级尺度。
  protectedPriorityFloor: number;
  protectedMemoryLevelFloor: number;
  skipProtectedSharedMemories: boolean;
}

// VmmNativeTextPolicySummary carries only the two editable intake reduction controls.
// VmmNativeTextPolicySummary 仅携带两个可编辑的入库缩减控制项。
export interface VmmNativeTextPolicySummary {
  // Enables machine folding without bypassing mandatory privacy and thought cleanup.
  // 启用机器文本折叠，不绕过强制隐私与思考内容清理。
  foldMachineText: boolean;
  // Approximate per-fragment token target for newly submitted content only.
  // 仅用于新提交内容的每片段近似 Token 目标。
  maxFragmentTokens: number;
}

// One finite profile level's capped refresh formula, in exact milliseconds.
// 一个有限画像等级的有上限刷新公式，使用准确毫秒值。
export interface VmmNativeProfileLifetimeRule {
  // Duration before refresh extension.
  // 刷新延长前的期限。
  baseMs: number;
  // Added duration per refresh-weight increment; zero disables extension.
  // 每次刷新权重增加的延长期限；零表示不延长。
  stepMs: number;
  // Maximum duration including extensions.
  // 包含延长量的最大期限。
  maxMs: number;
}
// Persistent is non-expiring and therefore has no editable lifetime rule.
// 持久级不过期，因此不提供可编辑的期限规则。
export interface VmmNativeProfileLifetimes {
  // Short-lived facts.
  // 短期事实。
  transient: VmmNativeProfileLifetimeRule;
  // Situational facts.
  // 情境事实。
  situational: VmmNativeProfileLifetimeRule;
  // Stable facts.
  // 稳定事实。
  stable: VmmNativeProfileLifetimeRule;
}
// Global fixed-user rules remain independent of project profile defaults.
// 全局固定用户规则独立于项目画像默认值。
export interface VmmNativeProfilePolicySummary {
  // Rules for the one fixed user.
  // 唯一固定用户的规则。
  user: VmmNativeProfileLifetimes;
  // Default rules for project profiles.
  // 项目画像的默认规则。
  project: VmmNativeProfileLifetimes;
}

// Project-only lifecycle defaults, selected by stable memory identity rather than host tag.
// 仅项目生命周期默认规则，按稳定记忆身份选择而非宿主标签。
export interface VmmNativeProjectProfilePolicySummary {
  // Existing memory-project identity.
  // 已存在的记忆项目身份。
  projectId: number;
  // Null explicitly inherits current global project defaults.
  // null 明确继承当前全局项目默认值。
  overrides: VmmNativeProfileLifetimes | null;
  // Validated formulas for the next profile-producing command.
  // 下一生成画像命令使用的已校验公式。
  effective: VmmNativeProfileLifetimes;
  // Global project rules only; personal profile rules are deliberately absent.
  // 仅全局项目规则；明确不包含个人画像规则。
  global: VmmNativeProfileLifetimes;
}

// Caller-owned groups never override memory-owner cleanup or the fixed user's profile rules.
// 调用方配置组绝不覆盖记忆归属清洗或固定用户画像规则。
export interface VmmNativeHostProjectPolicyOverrides {
  // Null follows the current global recall/extraction group.
  // 空值跟随当前全局召回/提炼组。
  recall: VmmNativeRecallPolicySummary | null;
  // Null follows the current global ranking group.
  // 空值跟随当前全局排序组。
  retrieval: VmmNativeRetrievalPolicySummary | null;
  // Null follows the current global intake cleanup group.
  // 空值跟随当前全局入库清洗组。
  text: VmmNativeTextPolicySummary | null;
}
// Read under one configuration gate with exact caller identity and explicit inheritance.
// 在同一配置锁内读取，具有准确调用方身份及显式继承状态。
export interface VmmNativeHostProjectPolicySummary {
  // Originating VulcanCode tag, not a VMM memory-project ID.
  // 发起方 VulcanCode 标签，不是 VMM 记忆项目 ID。
  hostProjectId: string;
  // Explicit independently optional groups.
  // 显式独立可选配置组。
  overrides: VmmNativeHostProjectPolicyOverrides;
  // Values resolved from current globals and only this caller's groups.
  // 从当前全局值和仅此调用方配置组解析的值。
  effective: VmmNativeRuntimePolicySummary;
}

// Effective native policy with exact defaults for independent draft reset.
// 原生有效策略，附带独立重置草稿使用的准确默认值。
export interface VmmNativeRuntimePolicySummary {
  // Current effective recall policy.
  // 当前有效的召回策略。
  recall: VmmNativeRecallPolicySummary;
  // Authoritative defaults for an explicit draft reset.
  // 显式重置草稿使用的权威默认值。
  defaults: VmmNativeRecallPolicySummary;
  // Effective ranking and Core-owned ranking defaults.
  // 有效排序设置和 Core 排序默认值。
  retrieval: VmmNativeRetrievalPolicySummary;
  retrievalDefaults: VmmNativeRetrievalPolicySummary;
  // Effective cleanup group and authoritative explicit-reset values.
  // 当前有效清洗组及权威显式重置值。
  retention: VmmNativeRetentionSettingsSummary;
  retentionDefaults: VmmNativeRetentionSettingsSummary;
  // Effective text reduction and Core-owned default values.
  // 当前有效文本缩减及 Core 持有的默认值。
  text: VmmNativeTextPolicySummary;
  textDefaults: VmmNativeTextPolicySummary;
  // Effective role-specific lifetimes and authoritative reset values.
  // 当前有效的角色专属生命周期及权威重置值。
  profiles: VmmNativeProfilePolicySummary;
  profileDefaults: VmmNativeProfilePolicySummary;
  // Whether a user runtime-policy document exists.
  // 是否存在用户运行策略文档。
  userOverride: boolean;
}

// Native resource composition remains separate from recall and extraction parameters.
// 原生资源组合保持独立于召回及提炼参数。
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

// VmmNativeRetentionPolicySummary is the frozen Core cleanup policy shown to users.
// VmmNativeRetentionPolicySummary 是向用户展示的冻结 Core 清洗策略。
export interface VmmNativeRetentionPolicySummary {
  schemaVersion: number;
  turnHotWindowSize: number;
  sessionIdleMs: number;
  trashRetentionMs: number;
  protectedPriorityFloor: number;
  protectedMemoryLevelFloor: number;
  skipProtectedSharedMemories: boolean;
  profileExpiryLimit: number;
  memoryRecycleLimit: number;
  coldTurnScanLimit: number;
  coldTurnJobLimit: number;
  idleSessionLimit: number;
  vectorGcLimit: number;
  trashPurgeLimit: number;
}

// VmmNativeRetentionApplyResult summarizes one complete native cleanup run.
// VmmNativeRetentionApplyResult 汇总一次完整原生清洗运行。
export interface VmmNativeRetentionApplyResult {
  expiredProfileNodes: number;
  recycledMemories: number;
  recycledTurns: number;
  enqueuedColdJobs: number;
  completedColdJobs: number;
  purgedTrashBatches: number;
  projectFailures: VmmNativeRetentionProjectFailure[];
}

// VmmNativeRetentionProjectFailure preserves one project-local cleanup failure.
// VmmNativeRetentionProjectFailure 保留一个项目局部清洗失败。
export interface VmmNativeRetentionProjectFailure {
  projectId: number;
  error: string;
}

// VmmNativeRetentionStatusSummary exposes the schedule and latest retention outcome.
// VmmNativeRetentionStatusSummary 暴露调度状态与最近保留结果。
export interface VmmNativeRetentionStatusSummary {
  enabled: boolean;
  running: boolean;
  profileExpiryRunning: boolean;
  intervalMinutes: number;
  profileExpiryIntervalSeconds: number;
  policy: VmmNativeRetentionPolicySummary;
  lastStartedAtUnixMs?: number | null;
  lastFinishedAtUnixMs?: number | null;
  lastError?: string | null;
  lastResult?: VmmNativeRetentionApplyResult | null;
  profileExpiryLastStartedAtUnixMs?: number | null;
  profileExpiryLastFinishedAtUnixMs?: number | null;
  profileExpiryLastError?: string | null;
  profileExpiryLastResult?: VmmNativeRetentionApplyResult | null;
  pendingColdJobs: number;
  recycleBatchCount: number;
}

// VmmNativeVectorStatusSummary reports active and recoverable memory compatibility with the configured embedding generation.
// VmmNativeVectorStatusSummary 报告活动与可恢复记忆和已配置 Embedding 代次的兼容状态。
export interface VmmNativeVectorStatusSummary {
  configured: boolean;
  currentVectorSpaceId?: string | null;
  embeddingDimension?: number | null;
  activeMemoryCount: number;
  recycleBinMemoryCount: number;
  currentMemoryCount: number;
  currentRecycleBinMemoryCount: number;
  rebuildRequiredMemoryCount: number;
  activeRebuildRequiredMemoryCount: number;
  recycleBinRebuildRequiredMemoryCount: number;
  legacyVectorSpaceCount: number;
  pendingAnalysisJobCount: number;
  error?: string | null;
}

// VmmNativeVectorRebuildPreview binds a high-impact rebuild to one exact displayed target set.
// VmmNativeVectorRebuildPreview 将高影响重建绑定到一个准确显示的目标集合。
export interface VmmNativeVectorRebuildPreview {
  storageVersion: number;
  targetVectorSpaceId: string;
  embeddingDimension: number;
  activeMemoryCount: number;
  recycleBinMemoryCount: number;
  rebuildMemoryCount: number;
  activeRebuildMemoryCount: number;
  recycleBinRebuildMemoryCount: number;
  affectedProjectCount: number;
  batchCount: number;
  previewFingerprint: string;
}

// VmmNativeVectorRebuildResult summarizes one completed native vector-generation rebuild.
// VmmNativeVectorRebuildResult 汇总一次已完成的原生向量代次重建。
export interface VmmNativeVectorRebuildResult {
  storageVersion: number;
  targetVectorSpaceId: string;
  rebuiltMemoryCount: number;
  batchCount: number;
}

// Reports source version and real derived full-text counts without asserting semantic integrity.
// 报告源版本与真实派生全文计数，不据此宣称语义完整。
export interface VmmNativeLexicalIndexStatus {
  // Authoritative snapshot version required at confirmation.
  // 确认时要求的权威快照版本。
  storageVersion: number;
  // Active lifecycle source count; access and expiry filters still run during queries.
  // 活跃生命周期源数量，查询时仍执行访问及到期过滤。
  activeMemoryCount: number;
  // Actual derived source-document and FTS document row counts.
  // 真实派生源文档及全文文档行数。
  documentCount: number;
  indexedRowCount: number;
}

// Returns the acknowledged full-text rebuild count and unchanged source version.
// 返回已确认的全文重建数量和未改变的源版本。
export interface VmmNativeLexicalRebuildResult {
  // Authority was not modified by derived reconstruction.
  // 派生重建未修改权威状态。
  storageVersion: number;
  // Actual rows reindexed by the upstream Jieba implementation.
  // 上游 Jieba 实现实际重建的行数。
  reindexedMemoryCount: number;
}

// Describes exact record identities affected by one project's confirmed cleanup stages.
// 描述一个项目确认清洗阶段影响的准确记录标识。
export interface VmmNativeRetentionProjectPreview {
  // Stable project identity and current display name.
  // 稳定项目标识与当前显示名称。
  projectId: number;
  projectName: string;
  // Recoverable memory and turn identities.
  // 可恢复的记忆和回合标识。
  recycledMemoryIds: string[];
  recycledTurnIds: string[];
  // Profile nodes marked expired, never permanently deleted by expiry convergence.
  // 标记过期的画像节点，到期收敛绝不永久删除它们。
  expiredProfileNodeIds: string[];
  // Existing expired batches to permanently remove.
  // 要永久移除的已有到期批次。
  purgedBatchIds: string[];
}

// Identifies the latest read-only server-prepared cleanup; client targets are never accepted.
// 标识最近的只读服务端清洗预览，绝不接受客户端自行指定目标。
export interface VmmNativeRetentionPreview {
  // Opaque confirmation token and exact source version.
  // 不透明确认令牌与准确源版本。
  previewId: string;
  storageVersion: number;
  // Frozen evaluation and confirmation expiry times, in Unix milliseconds.
  // 冻结评估与确认到期时间，以 Unix 毫秒表示。
  evaluatedAtUnixMs: number;
  expiresAtUnixMs: number;
  // Exact impact separated by memory project.
  // 按记忆项目分开的准确影响。
  projects: VmmNativeRetentionProjectPreview[];
}

// Selects exact native sessions for classification without modifying their content.
// 选择准确原生会话进行分类，不修改其内容。
export interface VmmNativeSessionArchiveSelection {
  // Exact owning memory project.
  // 准确归属的记忆项目。
  vmmProjectId: number;
  // Stable session identities, not display titles.
  // 稳定会话身份，而不是显示标题。
  sessionSourceIds: string[];
  // Archive when true; unarchive when false.
  // 为真时归档，为假时取消归档。
  archived: boolean;
}

// Selects forgotten sessions by exact owner and source IDs, not display names.
// 按准确归属及来源 ID 选择已遗忘会话，不使用显示名称。
export interface VmmNativeSessionRestartSelection {
  // Stable owning memory project.
  // 稳定归属记忆项目。
  vmmProjectId: number;
  // Exact identities normalized by the server.
  // 服务端归一化的准确身份。
  sessionSourceIds: string[];
}

// Describes the immutable restart preview shown before explicit confirmation.
// 描述明确确认前展示的不可变重新启用预览。
export interface VmmNativeSessionRestartPreview {
  // Opaque prepared confirmation identity.
  // 已准备确认的不透明身份。
  previewId: string;
  // Exact authoritative version.
  // 准确权威版本。
  storageVersion: number;
  // Prepared targets without replacement content.
  // 不含替代内容的已准备目标。
  selection: VmmNativeSessionRestartSelection;
}

// Reports reactivated targets without implying restoration of deleted content.
// 报告已重新启用目标，不表示恢复已删除内容。
export interface VmmNativeSessionRestartResult {
  // Committed authoritative version.
  // 已提交权威版本。
  storageVersion: number;
  // Exact committed targets.
  // 准确已提交目标。
  selection: VmmNativeSessionRestartSelection;
}

// Captures the latest server-prepared archive selection and observed version.
// 捕获最近服务端归档选择及观察版本。
export interface VmmNativeSessionArchivePreview {
  // Opaque server confirmation identity.
  // 服务端不透明确认身份。
  previewId: string;
  // Exact observed storage version.
  // 准确观察到的存储版本。
  storageVersion: number;
  // Immutable normalized selection.
  // 不可变归一化选择。
  selection: VmmNativeSessionArchiveSelection;
}

// Reports the exact committed classification and resulting storage version.
// 报告准确已提交分类及结果存储版本。
export interface VmmNativeSessionArchiveResult {
  // Committed authoritative version.
  // 已提交权威版本。
  storageVersion: number;
  // Unchanged preview selection.
  // 未被替换的预览选择。
  selection: VmmNativeSessionArchiveSelection;
}

// VmmNativeRecycleBatchSummary describes one recoverable native cleanup batch.
// VmmNativeRecycleBatchSummary 描述一个可恢复的原生清洗批次。
export interface VmmNativeRecycleBatchSummary {
  // Original selection includes empty sessions; null means an automatic retention batch.
  // 原始选择包含空会话；null 表示自动保留策略批次。
  manualSelection: VmmNativeRemovalTarget | null;
  batchId: string;
  projectId: number;
  reason: string;
  recycledAtUnixMs: number;
  expiresAtUnixMs: number;
  turnCount: number;
  memoryCount: number;
  profileNodeCount: number;
  contextEvidenceCount: number;
}

// Identifies the four retained business-record families exposed by batch inspection.
// 标识批次查看公开的四类保留业务记录。
export type VmmNativeRecycleRecordKind = "turn" | "memory" | "profile" | "evidence";

// Uses exact project/batch identity and version-bound pagination, never name matching.
// 使用准确项目、批次身份及版本绑定分页，不按名称匹配。
export interface VmmNativeRecycleRecordQuery {
  // Exact owning memory project.
  // 准确归属记忆项目。
  vmmProjectId: number;
  // Stable retained-batch identity.
  // 稳定保留批次身份。
  batchId: string;
  // Closed retained-record family.
  // 封闭保留记录类别。
  recordKind: VmmNativeRecycleRecordKind;
  // Zero-based stable-order offset and bounded page size.
  // 稳定顺序内从零开始的偏移及有界页大小。
  offset: number;
  limit: number;
  // Null explicitly starts a fresh observation; a number freezes subsequent pages.
  // Null 明确开始新观察，数字则冻结后续分页。
  expectedStorageVersion: number | null;
}

// Exact source selection for read-only impact inspection; never a deletion authorization.
// 用于只读影响检查的准确来源选择，绝不作为删除授权。
export type VmmNativeRemovalTarget = { kind: "sessions" | "turns"; ids: string[] };

// Freezes a server-owned selection; related independent profiles are preserved.
// 冻结服务端选择；关联独立画像保留。
export interface VmmNativeRemovalPreview {
  // Opaque single-use identity valid for five minutes.
  // 五分钟内有效的不透明单次身份。
  previewId: string;
  // Exact observed sources and hot-window impact.
  // 准确观察来源及热窗口影响。
  impact: VmmNativeRemovalImpact;
  // Owner-specific duration starting at the actual move.
  // 从实际移动时开始的归属专属保留时长。
  trashRetentionMs: number;
}

// Confirms only the server selection without sending replacement targets.
// 仅确认服务端选择，不发送替换目标。
export interface VmmNativeRemovalConfirmation {
  // Exact preview identity.
  // 准确预览身份。
  previewId: string;
  // Explicit consent to move the previewed original records.
  // 明确同意移动已预览的原记录。
  confirmed: boolean;
}

// Reports the durable record move and its original impact, not profile deletion.
// 报告耐久记录移动及其原始影响，而非画像删除。
export interface VmmNativeRemovalResult {
  // Version after the atomic move.
  // 原子移动后的版本。
  storageVersion: number;
  // Exact restore/purge batch identity.
  // 准确恢复或清除批次身份。
  batchId: string;
  // Executed source selection; listed profiles remain in active storage.
  // 已执行来源选择；列出的画像保留在活动存储。
  impact: VmmNativeRemovalImpact;
}

// Owner and selected canonical identities supplied to native source inspection.
// 提交给原生来源检查的归属及所选规范身份。
export interface VmmNativeRemovalQuery {
  // Stable memory-project owner and exact selected family.
  // 稳定记忆项目归属及准确所选类别。
  vmmProjectId: number;
  selection: VmmNativeRemovalTarget;
}

// Source relationships observed in one snapshot; profile lists remain candidates only.
// 单份快照中观察到的来源关系；画像列表仅为候选。
export interface VmmNativeRemovalImpact {
  // Normalized query, snapshot version and effective project hot window.
  // 归一化查询、快照版本及项目有效热窗口。
  query: VmmNativeRemovalQuery;
  storageVersion: number;
  hotWindowSize: number;
  // Accurate source sessions and canonical turns, including pending/hot subsets.
  // 准确来源会话及规范回合，包含待提炼和热窗口子集。
  sessionSourceIds: string[];
  turnIds: string[];
  pendingTurnIds: string[];
  hotWindowTurnIds: string[];
  // Ordinary memory identities and those declaring a vector generation, without vector payloads.
  // 普通记忆身份及声明向量代次的身份，不含向量载荷。
  memoryIds: string[];
  vectorMemoryIds: string[];
  // Related profile identities; these fields do not instruct profile deletion.
  // 关联画像身份，这些字段不指示删除画像。
  projectProfileNodeIds: string[];
  userProfileNodeIds: string[];
  // Complete composite keys of context edges owned by selected memories.
  // 所选记忆拥有的情境边完整复合键。
  contextEvidence: { memoryId: string; contextKey: string; contextValue: string }[];
}

// Contains retained text and source links only; embeddings never cross this UI contract.
// 仅包含保留文本及来源关系，嵌入向量不经过此界面契约。
export type VmmNativeRecycleRecord =
  | { kind: "turn"; turnId: string; sessionSourceId: string; timeline: VmmNativeTimelineItemSummary[]; status: string; analysisDetails: string | null }
  | { kind: "memory"; memoryId: string; sessionSourceId: string | null; sourceTurnId: string | null; summary: string; content: string; level: string; state: string; sourceKind: string }
  | { kind: "profile"; profileNodeId: string; target: "user" | "project"; sourceTurnIds: string[]; content: string }
  | { kind: "evidence"; memoryId: string; contextKey: string; contextValue: string; supportCount: number; rebuttalCount: number };

// Returns one coherent retained-content page without merging active records.
// 返回一致的保留内容页，不合并活动记录。
export interface VmmNativeRecycleRecordPage {
  // Exact echoed request and actual observed version.
  // 准确回显请求及实际观察版本。
  query: VmmNativeRecycleRecordQuery;
  storageVersion: number;
  // Family-wide count and an exact next offset, absent at the final page.
  // 类别总数及准确下一页偏移，最后一页时为空。
  total: number;
  nextOffset: number | null;
  // Retained business content without embedding vectors.
  // 不含嵌入向量的保留业务内容。
  records: VmmNativeRecycleRecord[];
}

// VmmNativeRecycleBinSummary is one version-guarded recycle-bin snapshot.
// VmmNativeRecycleBinSummary 是一份受版本保护的回收站快照。
export interface VmmNativeRecycleBinSummary {
  storageVersion: number;
  batches: VmmNativeRecycleBatchSummary[];
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
  | { kind: "native_session_list"; value: VmmNativeSessionListPage }
  | { kind: "native_profile_records"; value: VmmNativeProfileRecordsSummary }
  | { kind: "native_project_delete_preview"; value: VmmNativeProjectDeletePreview }
  | { kind: "native_migration_preview"; value: VmmNativeMigrationPreview }
  | { kind: "native_migration"; value: VmmNativeMigrationResult }
  | { kind: "native_clear_preview"; value: VmmNativeClearPreview }
  | { kind: "native_clear"; value: VmmNativeClearResult }
  | { kind: "native_resource_catalog"; value: VmmNativeResourceCatalogSummary }
  | { kind: "native_runtime_policy"; value: VmmNativeRuntimePolicySummary }
  | { kind: "native_host_project_policy"; value: VmmNativeHostProjectPolicySummary }
  | { kind: "native_project_retention"; value: VmmNativeProjectRetentionSummary }
  | { kind: "native_project_profile_policy"; value: VmmNativeProjectProfilePolicySummary }
  | { kind: "native_resource_file"; value: VmmNativeResourceFileContent }
  | { kind: "native_pii_test"; value: VmmNativePiiTestResult }
  | { kind: "native_pii_formula_test"; value: VmmNativePiiFormulaTestResult }
  | { kind: "native_noise_test"; value: VmmNativeNoiseTestResult }
  | { kind: "native_retention_status"; value: VmmNativeRetentionStatusSummary }
  | { kind: "native_retention_run"; value: VmmNativeRetentionApplyResult }
  | { kind: "native_retention_preview"; value: VmmNativeRetentionPreview }
  | { kind: "native_session_archive_preview"; value: VmmNativeSessionArchivePreview }
  | { kind: "native_session_archive"; value: VmmNativeSessionArchiveResult }
  | { kind: "native_session_restart_preview"; value: VmmNativeSessionRestartPreview }
  | { kind: "native_session_restart"; value: VmmNativeSessionRestartResult }
  | { kind: "native_lexical_status"; value: VmmNativeLexicalIndexStatus }
  | { kind: "native_lexical_rebuild"; value: VmmNativeLexicalRebuildResult }
  | { kind: "native_vector_status"; value: VmmNativeVectorStatusSummary }
  | { kind: "native_vector_rebuild_preview"; value: VmmNativeVectorRebuildPreview }
  | { kind: "native_vector_rebuild"; value: VmmNativeVectorRebuildResult }
  | { kind: "native_recycle_bin"; value: VmmNativeRecycleBinSummary }
  | { kind: "native_recycle_records"; value: VmmNativeRecycleRecordPage }
  | { kind: "native_removal_impact"; value: VmmNativeRemovalImpact }
  | { kind: "native_removal_preview"; value: VmmNativeRemovalPreview }
  | { kind: "native_removal"; value: VmmNativeRemovalResult }
  | { kind: "native_recycle_batch"; value: VmmNativeRecycleBatchSummary }
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
