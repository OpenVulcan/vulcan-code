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

// VmmMemorySnapshot describes the full VMM memory management page.
// VmmMemorySnapshot 描述完整 VMM 记忆管理页面。
export interface VmmMemorySnapshot {
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
  aiToolsEnabled: boolean;
  precheckEnabled: boolean;
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
  enabled: boolean;
  grpcTarget: string;
  grpcApiKey: string;
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
}

// VmmRuntimeSummary describes selected-session VMM runtime side effects.
// VmmRuntimeSummary 描述当前选中会话的 VMM 运行态副作用。
export interface VmmRuntimeSummary {
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
  action: string;
  scope: string;
  projectId?: string | null;
  payload?: unknown;
}

// VmmMemoryActionResult describes one VMM action response.
// VmmMemoryActionResult 描述单个 VMM 动作响应。
export interface VmmMemoryActionResult {
  action: string;
  summary: string;
  kind: string;
  request: unknown;
  response: unknown;
}

// VmmConfigSaveRequest describes one VMM config save request.
// VmmConfigSaveRequest 描述单个 VMM 配置保存请求。
export interface VmmConfigSaveRequest {
  scope: string;
  projectId?: string | null;
  draft: VmmConfigDraftSummary;
}
