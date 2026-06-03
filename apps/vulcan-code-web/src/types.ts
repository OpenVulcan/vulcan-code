// StartupStepStatus mirrors the Rust service API startup status enum.
// StartupStepStatus 对齐 Rust service API 的启动状态枚举。
export type StartupStepStatus = "pending" | "running" | "ready" | "warning" | "error";

// MainAreaId describes selectable top-level WebUI areas.
// MainAreaId 描述 WebUI 顶层可选择区域。
export type MainAreaId = "chat" | "skills" | "automation" | "plugins" | "memory" | "settings";

// SettingsSubAreaId describes secondary settings pages that absorb legacy top-level pages.
// SettingsSubAreaId 描述承接旧顶层页面的设置二级页面。
export type SettingsSubAreaId = "general" | "projects" | "providers" | "tools" | "diagnostics";

// StartupStepSnapshot describes one startup step displayed by the startup panel.
// StartupStepSnapshot 描述启动面板展示的单个启动步骤。
export interface StartupStepSnapshot {
  // Stable startup step identifier.
  // 稳定启动步骤标识。
  id: string;
  // User-facing startup step label.
  // 面向用户显示的启动步骤名称。
  label: string;
  // Current startup step status.
  // 当前启动步骤状态。
  status: StartupStepStatus;
  // Optional startup detail message.
  // 可选启动详情消息。
  message?: string | null;
}

// StartupSnapshot describes the full startup status returned by the service.
// StartupSnapshot 描述服务返回的完整启动状态。
export interface StartupSnapshot {
  // Overall readiness flag.
  // 整体就绪标记。
  ready: boolean;
  // Current startup phase.
  // 当前启动阶段。
  phase: string;
  // Ordered startup steps.
  // 有序启动步骤。
  steps: StartupStepSnapshot[];
  // Recent startup log lines.
  // 最近启动日志行。
  recentLogs: string[];
}

// RuntimeModuleSnapshot describes one runtime module card.
// RuntimeModuleSnapshot 描述单个运行时模块卡片。
export interface RuntimeModuleSnapshot {
  // Stable module identifier.
  // 稳定模块标识。
  id: string;
  // User-facing module label.
  // 面向用户显示的模块名称。
  label: string;
  // Current module status.
  // 当前模块状态。
  status: StartupStepStatus;
  // Short module message.
  // 简短模块消息。
  message: string;
}

// ProjectSummary describes one project shown in the sidebar.
// ProjectSummary 描述侧栏中展示的单个项目。
export interface ProjectSummary {
  // Stable project identifier.
  // 稳定项目标识。
  id: string;
  // Project display name.
  // 项目显示名称。
  name: string;
  // Project path.
  // 项目路径。
  path: string;
  // Whether the project is currently pinned.
  // 项目当前是否处于置顶状态。
  pinned?: boolean;
  // Optional icon pack selected for the project.
  // 项目选择的可选图标包。
  iconPack?: string | null;
  // Optional icon key selected for the project.
  // 项目选择的可选图标键。
  iconKey?: string | null;
}

// ConversationSummary describes one conversation shown in the sidebar.
// ConversationSummary 描述侧栏中展示的单个会话。
export interface ConversationSummary {
  // Stable conversation identifier.
  // 稳定会话标识。
  id: string;
  // Conversation title.
  // 会话标题。
  title: string;
  // Conversation status.
  // 会话状态。
  status: string;
  // Relative last-updated label.
  // 相对最后更新时间标签。
  lastUpdatedLabel?: string;
  // Raw last-updated timestamp as Unix milliseconds text.
  // 以 Unix 毫秒文本表示的原始最近更新时间。
  lastUpdatedAt?: string;
  // Whether the conversation has unread activity.
  // 会话是否存在未读活动。
  unread?: boolean;
  // Optional owning project identifier.
  // 可选所属项目标识。
  projectId?: string | null;
}

// ConversationMessage describes one rendered message in the timeline.
// ConversationMessage 描述时间线中的单条渲染消息。
export interface ConversationMessage {
  // Stable message identifier.
  // 稳定消息标识。
  id: string;
  // Stable conversation identifier that owns this message.
  // 拥有该消息的稳定会话标识。
  conversationId: string;
  // Message role.
  // 消息角色。
  role: "user" | "assistant" | "system";
  // Optional transcript title from the persisted session entry.
  // 来自持久化会话项的可选转录标题。
  title?: string | null;
  // Optional durable transcript record kind.
  // 可选持久化转录记录类型。
  recordKind?: string | null;
  // Message body.
  // 消息正文。
  body: string;
  // Optional structured transcript metadata.
  // 可选结构化转录元数据。
  metadata?: unknown;
}

// ToolCallSummary describes one tool call block.
// ToolCallSummary 描述单个工具调用块。
export interface ToolCallSummary {
  // Stable tool call identifier.
  // 稳定工具调用标识。
  id: string;
  // Tool display name.
  // 工具显示名称。
  name: string;
  // Tool status.
  // 工具状态。
  status: string;
  // Tool detail text.
  // 工具详情文本。
  detail: string;
}

// FeatureAreaSummary describes one primary navigation area.
// FeatureAreaSummary 描述单个主导航区域。
export interface FeatureAreaSummary {
  // Stable feature area identifier.
  // 稳定功能区域标识。
  id: MainAreaId;
  // Feature area display label.
  // 功能区域显示名称。
  label: string;
  // Feature area description.
  // 功能区域描述。
  description: string;
  // Feature area status.
  // 功能区域状态。
  status: string;
}

// PluginSummary describes one plugin shown in plugin center.
// PluginSummary 描述插件中心展示的单个插件。
export interface PluginSummary {
  // Stable plugin identifier.
  // 稳定插件标识。
  id: string;
  // Plugin display name.
  // 插件显示名称。
  name: string;
  // Plugin kind.
  // 插件类型。
  kind: string;
  // Plugin status.
  // 插件状态。
  status: string;
  // Plugin capability summary.
  // 插件能力摘要。
  capability: string;
}

// SettingSectionSummary describes one settings group.
// SettingSectionSummary 描述单个设置分组。
export interface SettingSectionSummary {
  // Stable settings section identifier.
  // 稳定设置分组标识。
  id: string;
  // Settings section display label.
  // 设置分组显示名称。
  label: string;
  // Settings section description.
  // 设置分组描述。
  description: string;
  // Representative setting item names.
  // 代表性设置项名称。
  items: string[];
}

// ProviderSummary describes one provider card.
// ProviderSummary 描述单个 Provider 卡片。
export interface ProviderSummary {
  // Stable provider identifier.
  // 稳定 Provider 标识。
  id: string;
  // Provider display name.
  // Provider 显示名称。
  name: string;
  // Provider status.
  // Provider 状态。
  status: string;
  // Enabled model count text.
  // 已启用模型数量文本。
  enabledModels: string;
  // Whether this provider owns the active workbench model.
  // 该 Provider 是否拥有当前激活的工作台模型。
  active?: boolean;
  // Enabled models exposed to the model picker.
  // 暴露给模型选择器的已启用模型。
  models?: ProviderModelSummary[];
}

// ProviderModelSummary describes one enabled model under a provider.
// ProviderModelSummary 描述 Provider 下的单个已启用模型。
export interface ProviderModelSummary {
  // Stable model identifier persisted with provider id.
  // 与 Provider 标识一起持久化的稳定模型标识。
  id: string;
  // Model display name.
  // 模型显示名称。
  name: string;
  // Whether this model is currently active.
  // 该模型当前是否处于激活状态。
  active?: boolean;
}

// MemorySummary describes one VMM memory area item.
// MemorySummary 描述单个 VMM 记忆区域项目。
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

// CommandSessionSummary describes one command session.
// CommandSessionSummary 描述单个命令会话。
export interface CommandSessionSummary {
  // Stable command session identifier.
  // 稳定命令会话标识。
  id: string;
  // Command text.
  // 命令文本。
  command: string;
  // Command status.
  // 命令状态。
  status: string;
  // Recent output preview.
  // 最近输出预览。
  preview: string;
}

// LogEntrySummary describes one diagnostic log entry.
// LogEntrySummary 描述单条诊断日志。
export interface LogEntrySummary {
  // Stable log entry identifier.
  // 稳定日志条目标识。
  id: string;
  // Log level.
  // 日志级别。
  level: string;
  // Log source.
  // 日志来源。
  source: string;
  // Log message.
  // 日志消息。
  message: string;
}

// WorkbenchSnapshot describes the first WebUI workbench state.
// WorkbenchSnapshot 描述第一版 WebUI 工作台状态。
export interface WorkbenchSnapshot {
  // Application version.
  // 应用版本。
  appVersion: string;
  // Runtime module cards.
  // 运行时模块卡片。
  modules: RuntimeModuleSnapshot[];
  // Project summaries.
  // 项目摘要。
  projects: ProjectSummary[];
  // Expanded project identifiers shown in the workspace sidebar.
  // 工作台侧栏中当前展开的项目标识集合。
  expandedProjectIds?: string[];
  // Conversation summaries.
  // 会话摘要。
  conversations: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId?: string | null;
  // Conversation messages.
  // 会话消息。
  messages: ConversationMessage[];
  // Tool call summaries.
  // 工具调用摘要。
  toolCalls: ToolCallSummary[];
  // Main feature areas.
  // 主功能区域。
  featureAreas: FeatureAreaSummary[];
  // Plugin summaries.
  // 插件摘要。
  plugins: PluginSummary[];
  // Settings sections.
  // 设置分组。
  settings: SettingSectionSummary[];
  // Provider summaries.
  // Provider 摘要。
  providers: ProviderSummary[];
  // VMM memory summaries.
  // VMM 记忆摘要。
  memories: MemorySummary[];
  // Command session summaries.
  // 命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[];
}
