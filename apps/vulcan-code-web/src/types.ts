// StartupStepStatus mirrors the Rust service API startup status enum.
// StartupStepStatus 对齐 Rust service API 的启动状态枚举。
export type StartupStepStatus = "pending" | "running" | "ready" | "warning" | "error";

// MainAreaId describes selectable top-level WebUI areas.
// MainAreaId 描述 WebUI 顶层可选择区域。
export type MainAreaId = "chat" | "skills" | "memory" | "settings";

// SettingsSubAreaId describes secondary settings pages aligned with the legacy iced settings tree.
// SettingsSubAreaId 描述与旧 iced 设置树对齐的设置二级页面。
export type SettingsSubAreaId =
  | "model-providers"
  | "model-list"
  | "ai-vendors"
  | "ai-web-search"
  | "ai-understand-image"
  | "ai-understand-video"
  | "ai-understand-audio"
  | "ai-image-generation"
  | "ai-video-generation"
  | "ai-speech-synthesis"
  | "ai-music-generation"
  | "ai-music-cover"
  | "mcp-servers"
  | "runtime-diagnostics"
  | "prompt-composer"
  | "about-licenses"
  | "archived-restore";

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

// AiCapabilityTargetSummary describes one configured AI capability route target.
// AiCapabilityTargetSummary 描述单个已配置的 AI 能力路由目标。
export interface AiCapabilityTargetSummary {
  // Stable vendor/target selector used by prompt-level overrides.
  // 提示词级覆盖使用的稳定 vendor/target 选择器。
  targetSelector: string;
  // Provider or capability vendor identifier.
  // Provider 或能力供应商标识。
  vendorId: string;
  // User-facing vendor name.
  // 面向用户显示的供应商名称。
  vendorName: string;
  // Route target alias exposed to the runtime.
  // 暴露给运行时的路由目标别名。
  targetAlias: string;
  // User-facing target name.
  // 面向用户显示的目标名称。
  targetName: string;
  // Provider model identifier used by this target.
  // 当前目标使用的 Provider 模型标识。
  modelId: string;
  // Whether this target is globally enabled.
  // 当前目标是否全局启用。
  enabled: boolean;
}

// AiCapabilitySummary describes one semantic AI capability exposed by the composer.
// AiCapabilitySummary 描述输入区暴露的单个语义 AI 能力。
export interface AiCapabilitySummary {
  // Stable semantic capability key.
  // 稳定语义能力键。
  kind: string;
  // User-facing capability label.
  // 面向用户显示的能力名称。
  label: string;
  // Whether the capability has at least one usable route.
  // 当前能力是否至少拥有一条可用路由。
  available: boolean;
  // Whether the capability is globally enabled.
  // 当前能力是否全局启用。
  enabled: boolean;
  // Whether the active provider/model handles this capability natively.
  // 当前 Provider/模型是否原生处理该能力。
  native: boolean;
  // Whether host-side AI tools bridge this capability.
  // 宿主侧 AI 工具是否桥接该能力。
  bridged: boolean;
  // Configured targets under this capability.
  // 当前能力下已配置的目标。
  targets: AiCapabilityTargetSummary[];
}

// AiCapabilityVendorSummary describes one shared AI capability vendor.
// AiCapabilityVendorSummary 描述单个共享 AI 能力供应商。
export interface AiCapabilityVendorSummary {
  // Stable vendor instance id.
  // 稳定供应商实例标识。
  id: string;
  // Stable catalog vendor id.
  // 稳定目录供应商标识。
  vendorId: string;
  // Vendor display name.
  // 供应商显示名称。
  name: string;
  // Whether the vendor is enabled.
  // 供应商是否启用。
  enabled: boolean;
  // Stored API key or environment placeholder.
  // 已保存 API Key 或环境变量占位符。
  apiKey: string;
  // Stored base URL override.
  // 已保存 Base URL 覆盖值。
  baseUrl: string;
  // Stored region override.
  // 已保存区域覆盖值。
  region: string;
  // Stored connection mode.
  // 已保存连接模式。
  connectionMode: string;
  // Configured target count.
  // 已配置目标数量。
  targetCount: number;
  // Catalog target choices exposed by this vendor.
  // 该供应商暴露的目录目标选项。
  targetChoices?: AiCapabilityVendorTargetChoiceSummary[];
}

// AiCapabilityVendorTargetChoiceSummary describes one selectable target below a configured vendor.
// AiCapabilityVendorTargetChoiceSummary 描述已配置供应商下单个可选择目标。
export interface AiCapabilityVendorTargetChoiceSummary {
  // Stable semantic capability kind owned by this target.
  // 当前目标所属的稳定语义能力类型。
  capabilityKind: string;
  // Stable target alias under the vendor.
  // 供应商下的稳定目标别名。
  targetAlias: string;
  // Default provider request model attached to the target.
  // 当前目标默认绑定的 Provider 请求模型。
  defaultModelId: string;
  // User-facing compact label.
  // 面向用户的紧凑标签。
  label: string;
  // Whether this target supports editing request model.
  // 当前目标是否支持编辑请求模型。
  supportsModelOverride: boolean;
  // Optional finite model choices.
  // 可选的有限模型选项。
  modelOptions: string[];
}

// AiCapabilityVendorCatalogSummary describes one supported vendor picker choice.
// AiCapabilityVendorCatalogSummary 描述单个受支持供应商选择项。
export interface AiCapabilityVendorCatalogSummary {
  // Stable vendor identifier.
  // 稳定供应商标识。
  vendorId: string;
  // Vendor display name.
  // 供应商显示名称。
  name: string;
  // Supported capability labels.
  // 支持的能力标签。
  capabilities: string[];
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

// ProjectDirectoryEntrySummary describes one child directory returned by the service.
// ProjectDirectoryEntrySummary 描述服务返回的单个子目录。
export interface ProjectDirectoryEntrySummary {
  // Directory display name.
  // 目录显示名称。
  name: string;
  // Absolute path on the service host.
  // 服务宿主上的绝对路径。
  path: string;
  // Whether the directory is hidden by name convention.
  // 目录是否符合隐藏命名约定。
  hidden: boolean;
}

// ProjectDirectoryRootSummary describes one filesystem root returned by the service.
// ProjectDirectoryRootSummary 描述服务返回的单个文件系统根。
export interface ProjectDirectoryRootSummary {
  // Root display label.
  // 根目录显示标签。
  label: string;
  // Absolute root path on the service host.
  // 服务宿主上的绝对根路径。
  path: string;
}

// ProjectDirectoryListing describes one browsed service-host directory.
// ProjectDirectoryListing 描述一个已浏览的服务宿主目录。
export interface ProjectDirectoryListing {
  // Selectable filesystem roots.
  // 可选择的文件系统根。
  roots?: ProjectDirectoryRootSummary[];
  // Current absolute directory path.
  // 当前绝对目录路径。
  path: string;
  // Optional parent directory path.
  // 可选父目录路径。
  parentPath?: string | null;
  // Whether the current directory can be selected.
  // 当前目录是否可以选择。
  canSelect: boolean;
  // Child directory rows.
  // 子目录行。
  entries: ProjectDirectoryEntrySummary[];
  // Optional listing notice.
  // 可选列表提示。
  notice?: string | null;
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
  // Current per-session runtime phase.
  // 当前会话级运行阶段。
  runPhase?: string;
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
  // Unix-millisecond creation timestamp used for turn duration labels.
  // 用于轮次耗时标签的 Unix 毫秒创建时间戳。
  createdAt?: string;
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
  // Structured user attachments projected from transcript metadata.
  // 从转录元数据投影出的结构化用户附件。
  attachments?: ComposerAttachmentSummary[];
}

// AppendMessageBodyDelta appends streamed text to one visible conversation message.
// AppendMessageBodyDelta 向单条可见会话消息追加流式文本。
export interface AppendMessageBodyDelta {
  // Stable conversation identifier that owns the message.
  // 拥有该消息的稳定会话标识。
  conversationId: string;
  // Stable message identifier receiving the appended text.
  // 接收追加文本的稳定消息标识。
  messageId: string;
  // Text appended by this event only.
  // 本事件单独追加的文本片段。
  bodyDelta: string;
  // Full message body after this delta, preferred over local append reconstruction.
  // 本次增量后的完整消息正文，优先于本地追加复原。
  currentBody?: string | null;
  // Initial full message metadata when the row does not exist yet.
  // 当消息行尚不存在时使用的初始完整消息元数据。
  initialMessage?: ConversationMessage | null;
  // Current backend run phase after this delta.
  // 本次增量后的后端运行阶段。
  runPhase?: string | null;
}

// WorkbenchDelta describes one partial workbench update emitted on hot paths.
// WorkbenchDelta 描述热路径上发出的单条局部工作台更新。
export type WorkbenchDelta =
  | { kind: "appendMessageBody"; payload: AppendMessageBodyDelta };

// WorkbenchDeltaEnvelope carries one ordered partial workbench update.
// WorkbenchDeltaEnvelope 携带一条有序的局部工作台更新。
export interface WorkbenchDeltaEnvelope {
  // Monotonic backend generation used for diagnostics and ordering.
  // 用于诊断与排序的后端单调版本号。
  generation: number;
  // Partial workbench update payload.
  // 局部工作台更新载荷。
  delta: WorkbenchDelta;
}

// SessionAddonChangedEvent describes one persisted session addon-state namespace change.
// SessionAddonChangedEvent 描述一次已持久化的会话附属状态命名空间变化。
export interface SessionAddonChangedEvent {
  // Session identifier whose addon state changed.
  // 附属状态发生变化的会话标识。
  sessionId: string;
  // Changed addon namespace keys.
  // 已变化的附属状态命名空间键。
  namespaces: string[];
  // Latest addon document revision after the change.
  // 变化后的最新附属状态文档版本。
  revision: number;
  // Unix-millis text representing the latest addon update time.
  // 表示最近附属状态更新时间的 Unix 毫秒文本。
  updatedAt: string;
  // Optional local instance id that originated the change.
  // 发起该变化的可选本地实例标识。
  sourceInstanceId?: string | null;
}

// ComposerAttachmentCapabilitySummary describes whether one attachment kind is allowed.
// ComposerAttachmentCapabilitySummary 描述单种附件类型当前是否允许使用。
export interface ComposerAttachmentCapabilitySummary {
  // Stable attachment kind key.
  // 稳定附件类型键。
  kind: string;
  // User-facing attachment kind label.
  // 面向用户显示的附件类型名称。
  label: string;
  // Whether this attachment kind can be submitted.
  // 当前附件类型是否允许提交。
  enabled: boolean;
  // Whether the active provider/model supports this kind natively.
  // 当前 Provider/模型是否原生支持该类型。
  native: boolean;
  // Whether host-side AI tools bridge this kind.
  // 宿主侧 AI 工具是否桥接该类型。
  bridged: boolean;
  // Browser file picker accept string.
  // 浏览器文件选择 accept 字符串。
  accept: string;
}

// ComposerAttachmentSummary describes one staged composer attachment.
// ComposerAttachmentSummary 描述单条已暂存的输入区附件。
export interface ComposerAttachmentSummary {
  // Stable attachment kind key.
  // 稳定附件类型键。
  kind: string;
  // Human-readable file name.
  // 人类可读文件名。
  fileName: string;
  // Managed local path used for provider replay.
  // Provider 回放使用的托管本地路径。
  localPath: string;
  // Optional original source path.
  // 可选原始来源路径。
  sourcePath?: string | null;
  // Optional asset identifier.
  // 可选资源标识。
  assetId?: string | null;
  // Optional byte length.
  // 可选字节长度。
  byteLen?: number | null;
  // Optional preview local path.
  // 可选预览本地路径。
  previewLocalPath?: string | null;
  // MIME type.
  // MIME 类型。
  mimeType: string;
}

// PendingPromptSummary describes one visible queued prompt above the composer.
// PendingPromptSummary 描述输入区上方单条可见排队提示词。
export interface PendingPromptSummary {
  // Stable entry identifier.
  // 稳定条目标识。
  id: string;
  // Full prompt text restored during editing.
  // 编辑时恢复的完整提示词文本。
  prompt: string;
  // Compact queue preview.
  // 紧凑队列预览。
  preview: string;
  // Attachments replayed with the prompt.
  // 随提示词回放的附件。
  attachments: ComposerAttachmentSummary[];
  // Stable queue state key.
  // 稳定队列状态键。
  state: "ready" | "inserted" | string;
  // Unix-millis creation timestamp text.
  // Unix 毫秒格式创建时间文本。
  createdAt: string;
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

// LuaSkillToolSummary describes one Lua Skill runtime tool row.
// LuaSkillToolSummary 描述单个 Lua Skill 运行时工具行。
export interface LuaSkillToolSummary {
  // Canonical host-visible tool name.
  // 宿主可见的规范工具名称。
  canonicalName: string;
  // Local tool name inside the skill package.
  // 技能包内部的本地工具名称。
  localName: string;
  // Tool description from the runtime descriptor.
  // 来自运行时描述符的工具说明。
  description: string;
  // Persisted registration state key.
  // 持久化注册状态键。
  registrationState: "explicit" | "implicit" | "disabled";
  // Human-readable registration state label.
  // 人类可读的注册状态标签。
  registrationLabel: string;
  // Short help text for the current registration state.
  // 当前注册状态的简短说明。
  registrationHelp: string;
}

// LuaSkillSummary describes one Lua Skill package.
// LuaSkillSummary 描述单个 Lua Skill 包。
export interface LuaSkillSummary {
  // Stable skill identifier.
  // 稳定技能标识。
  skillId: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  displayName: string;
  // Skill package version.
  // 技能包版本。
  version: string;
  // Runtime root layer name.
  // 运行时根层名称。
  rootName: string;
  // Filesystem directory that contains the skill.
  // 包含该技能的文件系统目录。
  skillDir: string;
  // Whether this skill is currently enabled.
  // 该技能当前是否已启用。
  enabled: boolean;
  // Optional disabled reason.
  // 可选停用原因。
  disabledReason?: string | null;
  // Whether this skill has a managed install record.
  // 该技能是否拥有受管安装记录。
  managed: boolean;
  // Optional install source type label.
  // 可选安装来源类型标签。
  sourceType?: string | null;
  // Optional install source locator.
  // 可选安装来源定位值。
  sourceLocator?: string | null;
  // Number of visible runtime entries.
  // 可见运行时入口数量。
  entryCount: number;
  // Whether this skill belongs to the host-owned ROOT layer.
  // 该技能是否属于宿主自有 ROOT 层。
  systemOwned: boolean;
  // Tool rows exposed by this skill.
  // 该技能暴露的工具行。
  tools: LuaSkillToolSummary[];
}

// LuaSkillsCatalogSummary describes the full Lua Skills catalog.
// LuaSkillsCatalogSummary 描述完整 Lua Skills 目录。
export interface LuaSkillsCatalogSummary {
  // Runtime root used by the embedded Lua Skills host.
  // 嵌入式 Lua Skills 宿主使用的运行时根目录。
  runtimeRoot: string;
  // System Lua library directory used by Lua Skills.
  // Lua Skills 使用的系统 Lua 库目录。
  systemLuaLib: string;
  // Total visible runtime entry count.
  // 可见运行时入口总数。
  entryCount: number;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Skill package rows visible to the host.
  // 宿主当前可见的技能包行。
  skills: LuaSkillSummary[];
}

// AgentSkillSummary describes one prompt-based Agent Skill.
// AgentSkillSummary 描述单个基于提示词的 Agent Skill。
export interface AgentSkillSummary {
  // Stable skill name from frontmatter.
  // 来自 frontmatter 的稳定技能名称。
  name: string;
  // Human-facing display name.
  // 面向用户的显示名称。
  displayName: string;
  // Human-facing short description.
  // 面向用户的简短说明。
  description: string;
  // Source scope label.
  // 来源作用域标签。
  scope: string;
  // Whether this Agent Skill is enabled.
  // 该 Agent Skill 是否已启用。
  enabled: boolean;
  // Filesystem path to the SKILL.md file.
  // 指向 SKILL.md 文件的文件系统路径。
  skillFile: string;
  // Directory that owns this Agent Skill and bundled resources.
  // 拥有该 Agent Skill 与附带资源的目录。
  baseDir: string;
  // Optional resolved icon path.
  // 可选解析图标路径。
  iconPath?: string | null;
  // Markdown body without YAML frontmatter.
  // 去除 YAML frontmatter 后的 Markdown 正文。
  body: string;
}

// AgentSkillLoadIssueSummary describes one non-fatal Agent Skill loading issue.
// AgentSkillLoadIssueSummary 描述单个非致命 Agent Skill 加载问题。
export interface AgentSkillLoadIssueSummary {
  // Path related to the loading issue.
  // 与加载问题相关的路径。
  path: string;
  // Human-readable diagnostic message.
  // 人类可读诊断信息。
  message: string;
}

// AgentSkillsCatalogSummary describes the full Agent Skills catalog.
// AgentSkillsCatalogSummary 描述完整 Agent Skills 目录。
export interface AgentSkillsCatalogSummary {
  // Whether Agent Skills are globally enabled.
  // Agent Skills 是否已全局启用。
  globalEnabled: boolean;
  // System skill root used by host-managed Agent Skills.
  // 宿主管理 Agent Skills 使用的系统技能根目录。
  systemRoot: string;
  // User skill root used by user-installed Agent Skills.
  // 用户安装 Agent Skills 使用的用户技能根目录。
  userRoot: string;
  // Human-readable notice for the current catalog state.
  // 当前目录状态的人类可读提示。
  notice: string;
  // Parsed Agent Skill rows.
  // 已解析的 Agent Skill 行。
  skills: AgentSkillSummary[];
  // Non-fatal loading issues.
  // 非致命加载问题。
  errors: AgentSkillLoadIssueSummary[];
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
  // Whether this provider has a saved local configuration.
  // 该 Provider 是否已保存本地配置。
  configured?: boolean;
  // Enabled model count text.
  // 已启用模型数量文本。
  enabledModels: string;
  // Whether this provider owns the active workbench model.
  // 该 Provider 是否拥有当前激活的工作台模型。
  active?: boolean;
  // Enabled models exposed to the model picker.
  // 暴露给模型选择器的已启用模型。
  models?: ProviderModelSummary[];
  // Full catalog model list before enabled filtering.
  // 启用过滤前的完整目录模型列表。
  catalogModels?: ProviderModelSummary[];
  // Editable provider fields.
  // 可编辑 Provider 字段。
  fields?: ProviderFieldSummary[];
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
  // Whether this model is enabled.
  // 该模型是否启用。
  enabled?: boolean;
  // Optional model status.
  // 可选模型状态。
  status?: string | null;
  // Optional model family.
  // 可选模型家族。
  family?: string | null;
  // Optional context window limit.
  // 可选上下文窗口上限。
  contextLimit?: number | null;
  // Optional input token limit.
  // 可选输入 Token 上限。
  inputLimit?: number | null;
  // Optional output token limit.
  // 可选输出 Token 上限。
  outputLimit?: number | null;
  // Optional reasoning capability flag.
  // 可选推理能力标记。
  reasoning?: boolean | null;
  // Optional attachment capability flag.
  // 可选附件能力标记。
  attachment?: boolean | null;
  // Input modalities accepted by the model.
  // 模型接受的输入模态集合。
  inputModalities?: string[];
  // Output modalities emitted by the model.
  // 模型输出的模态集合。
  outputModalities?: string[];
  // Optional reasoning profile JSON text.
  // 可选推理档位配置 JSON 文本。
  reasoningProfileJson?: string | null;
}

// ProviderFieldSummary describes one editable provider field.
// ProviderFieldSummary 描述单个可编辑 Provider 字段。
export interface ProviderFieldSummary {
  // Stable field key.
  // 稳定字段键。
  key: string;
  // Field display label.
  // 字段显示名称。
  label: string;
  // Field placeholder text.
  // 字段占位提示。
  placeholder: string;
  // Stored field value.
  // 已保存字段值。
  value: string;
  // Whether this field should use password input.
  // 该字段是否使用密码输入。
  secret: boolean;
  // Whether this field is required.
  // 该字段是否必填。
  required: boolean;
}

// KeyValueSummary describes one string key/value pair.
// KeyValueSummary 描述单个字符串键值对。
export interface KeyValueSummary {
  // Stable key.
  // 稳定键。
  key: string;
  // Stored value.
  // 已保存值。
  value: string;
}

// McpServerSummary describes one persisted MCP server.
// McpServerSummary 描述单个持久化 MCP 服务器。
export interface McpServerSummary {
  // Stable server identifier.
  // 稳定服务器标识。
  id: string;
  // Server display name.
  // 服务器显示名称。
  name: string;
  // Whether this server is enabled.
  // 该服务器是否启用。
  enabled: boolean;
  // Transport key.
  // 传输键。
  transport: string;
  // Stdio command.
  // Stdio 启动命令。
  command: string;
  // Stdio arguments.
  // Stdio 参数。
  args: string[];
  // Remote endpoint URL.
  // 远端端点 URL。
  url: string;
  // Remote bearer token.
  // 远端 Bearer Token。
  bearerToken: string;
  // Explicit request headers.
  // 显式请求头。
  headers: KeyValueSummary[];
  // Request headers resolved from environment variables.
  // 从环境变量解析的请求头。
  headersFromEnv: KeyValueSummary[];
  // OAuth client id.
  // OAuth 客户端标识。
  oauthClientId: string;
  // OAuth client secret.
  // OAuth 客户端密钥。
  oauthClientSecret: string;
  // OAuth scopes.
  // OAuth scope 列表。
  oauthScopes: string[];
  // OAuth redirect URI.
  // OAuth 回调 URI。
  oauthRedirectUri: string;
  // Stdio working directory.
  // Stdio 工作目录。
  cwd: string;
  // Stdio environment variables.
  // Stdio 环境变量。
  env: KeyValueSummary[];
  // Stdio environment passthrough names.
  // Stdio 环境变量透传名称。
  envPassthrough: string[];
}

// ReasoningSelectionOptionSummary describes one backend-resolved reasoning selector option.
// ReasoningSelectionOptionSummary 描述单个后端解析出的思考选择器选项。
export interface ReasoningSelectionOptionSummary {
  // Stable reasoning effort value sent through the service router.
  // 通过服务路由发送的稳定思考档位值。
  value: string;
  // Provider-resolved user-facing option label.
  // Provider 解析出的面向用户选项标签。
  label: string;
}

// ReasoningSelectionSummary describes the active backend reasoning selector view.
// ReasoningSelectionSummary 描述当前激活的后端思考选择器视图。
export interface ReasoningSelectionSummary {
  // Dynamic options supported by the active provider and model.
  // 当前激活 Provider 与模型支持的动态选项。
  options: ReasoningSelectionOptionSummary[];
  // Effective selected reasoning effort value.
  // 当前生效的已选思考档位值。
  selectedEffort: string;
  // Optional backend reconciliation note.
  // 可选后端档位协调说明。
  note?: string | null;
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

// ContextInjectionItemSummary describes one visible injected context item.
// ContextInjectionItemSummary 描述单条可见注入上下文条目。
export interface ContextInjectionItemSummary {
  // Compact content preview displayed in the side panel.
  // 侧面板展示的紧凑内容预览。
  content: string;
  // Full detail text displayed as expanded context information.
  // 展开上下文信息时展示的完整详情文本。
  detail: string;
  // Optional turn label that owns this injection.
  // 拥有该注入的可选轮次标签。
  turnLabel?: string | null;
  // TTL or persistence label.
  // TTL 或持久化状态标签。
  ttlLabel: string;
  // Optional source display label.
  // 可选来源显示标签。
  sourceDisplay?: string | null;
  // Optional stable injection dedupe key.
  // 可选稳定注入去重键。
  dedupeKey?: string | null;
  // Optional durable VMM memory id.
  // 可选长期 VMM 记忆标识。
  memoryId?: number | null;
  // Optional source turn id.
  // 可选来源回合标识。
  turnId?: number | null;
  // Optional relevance score label.
  // 可选相关性评分标签。
  scoreLabel?: string | null;
  // Whether full turn detail can be queried.
  // 是否可以查询完整回合详情。
  hasDialogue?: boolean;
  // Optional VMM creation datetime.
  // 可选 VMM 创建时间。
  createdDatetime?: string | null;
  // Optional injection update timestamp.
  // 可选注入更新时间戳。
  updatedAt?: string | null;
}

// ContextInjectionCardSummary describes one context injection source group.
// ContextInjectionCardSummary 描述单个上下文注入来源分组。
export interface ContextInjectionCardSummary {
  // Stable context injection tag.
  // 稳定上下文注入标记。
  tag: string;
  // Human-readable card description.
  // 面向用户的卡片说明。
  description: string;
  // Items emitted by this injection source.
  // 当前注入来源产生的条目。
  items: ContextInjectionItemSummary[];
}

// VmmStaticProfileNodeSummary describes one active static VMM profile node.
// VmmStaticProfileNodeSummary 描述单个 active 静态 VMM 画像节点。
export interface VmmStaticProfileNodeSummary {
  // Durable VMM profile node id.
  // 长期 VMM 画像节点标识。
  id: number;
  // Priority marker such as P1.
  // 优先级标记，例如 P1。
  priority: string;
  // Lifetime marker such as L2.
  // 生命周期标记，例如 L2。
  level: string;
  // Refresh weight marker such as W3.
  // 刷新权重标记，例如 W3。
  refreshWeight: string;
  // Atomic profile content.
  // 原子化画像内容。
  content: string;
  // Source kind key returned by VMM.
  // VMM 返回的来源类型键。
  sourceKind: string;
  // Source row id such as the originating turn id.
  // 来源行标识，例如来源回合标识。
  sourceId: number;
  // Bound user/project/team/space id.
  // 绑定的 user/project/team/space 标识。
  bindId: number;
}

// VmmStaticProfileDateGroupSummary describes one date bucket inside a VMM profile section.
// VmmStaticProfileDateGroupSummary 描述 VMM 画像段落内的单个日期分组。
export interface VmmStaticProfileDateGroupSummary {
  // Human-readable profile date.
  // 人类可读画像日期。
  date: string;
  // Active profile nodes for this date.
  // 当前日期下的 active 画像节点。
  nodes: VmmStaticProfileNodeSummary[];
}

// VmmStaticProfileSectionSummary describes one target-scope static profile section.
// VmmStaticProfileSectionSummary 描述单个目标作用域的静态画像段落。
export interface VmmStaticProfileSectionSummary {
  // Stable target key such as team, space, project, or user.
  // 稳定目标键，例如 team、space、project 或 user。
  target: string;
  // Uppercase target label displayed by the panel.
  // 面板展示的大写目标标签。
  label: string;
  // Date groups under this target.
  // 当前目标下的日期分组。
  dates: VmmStaticProfileDateGroupSummary[];
}

// VmmStaticProfileSummary describes the current active VMM profile snapshot.
// VmmStaticProfileSummary 描述当前 active VMM 画像快照。
export interface VmmStaticProfileSummary {
  // Optional update timestamp captured by the backend.
  // 后端捕获的可选更新时间。
  updatedAt?: string | null;
  // Profile sections grouped by VMM target scope.
  // 按 VMM 目标作用域分组的画像段落。
  sections: VmmStaticProfileSectionSummary[];
}

// ContextInjectionSnapshot describes dynamic and static context injection panels.
// ContextInjectionSnapshot 描述动态与静态上下文注入面板数据。
export interface ContextInjectionSnapshot {
  // Dynamic per-turn injection cards.
  // 按轮次生效的动态注入卡片。
  dynamicCards: ContextInjectionCardSummary[];
  // Structured active VMM profile snapshot for static memory display.
  // 用于静态记忆展示的结构化 active VMM 画像快照。
  staticProfile?: VmmStaticProfileSummary | null;
  // Static persistent injection cards.
  // 持续生效的静态注入卡片。
  staticCards: ContextInjectionCardSummary[];
}

// ContextUsageSnapshot describes the composer context usage orb state.
// ContextUsageSnapshot 描述输入区上下文占用圆盘状态。
export interface ContextUsageSnapshot {
  // Rounded usage percent shown inside the orb.
  // 圆盘中显示的四舍五入占用百分比。
  percent: number;
  // Severity key used by WebUI styling.
  // WebUI 样式使用的严重级别键。
  severity: string;
  // Whether the active runtime still matches the accepted baseline.
  // 当前运行时是否仍匹配已接受的基线。
  baselineMatchesActiveRuntime: boolean;
  // Tooltip title.
  // 提示浮层标题。
  tooltipTitle: string;
  // Tooltip diagnostic rows.
  // 提示浮层诊断行。
  tooltipRows: string[];
}

// ComposerSlashCommandSummary describes one slash command routed by the shared workbench service.
// ComposerSlashCommandSummary 描述由共享工作台服务路由的单个 slash 指令。
export interface ComposerSlashCommandSummary {
  // Stable command identifier used by WebUI routing.
  // WebUI 路由使用的稳定指令标识。
  id: string;
  // Canonical English slash trigger shown in the composer.
  // 输入区展示的规范英文 slash 触发词。
  command: string;
  // Chinese command description.
  // 中文指令说明。
  descriptionZh: string;
  // English command description.
  // 英文指令说明。
  descriptionEn: string;
}

// SubagentNavigationSnapshot describes the backend-routed subagent surface state.
// SubagentNavigationSnapshot 描述由后端路由的子代理界面状态。
export interface SubagentNavigationSnapshot {
  // Parent conversation that owns the subagent task cards.
  // 拥有子代理任务卡片的父会话。
  parentConversationId: string;
  // Conversation id currently rendered by the transcript surface.
  // 当前由转录界面渲染的会话标识。
  activeConversationId: string;
  // Active child task id when a subagent surface is open.
  // 子代理界面打开时的活动子任务标识。
  activeTaskId?: string | null;
  // Ordered subagent tasks projected by the backend.
  // 由后端投影的有序子代理任务。
  tasks?: SubagentTaskSummary[];
}

// SubagentTaskSummary describes one subagent navigation row.
// SubagentTaskSummary 描述单条子代理导航行。
export interface SubagentTaskSummary {
  // Stable child task conversation id.
  // 稳定子任务会话标识。
  taskId: string;
  // Caller-facing task id retained in task-session metadata.
  // 保留在 task-session 元数据中的调用方面向任务标识。
  publicTaskId: string;
  // Human-facing label built by the shared backend projection.
  // 由共享后端投影构建的人类可读标签。
  label: string;
  // Optional task description.
  // 可选任务描述。
  description?: string | null;
  // Canonical run state key.
  // 规范运行状态键。
  runState: string;
  // Whether the shared task runtime reports this task as running.
  // 共享任务运行时是否报告该任务正在运行。
  isRunning: boolean;
  // Whether this task is currently opened.
  // 该任务当前是否已打开。
  isActive: boolean;
}

// PendingQuestionSnapshot describes one ask-tool request waiting for user input.
// PendingQuestionSnapshot 描述一个正在等待用户输入的 ask 工具请求。
export interface PendingQuestionSnapshot {
  // Stable request id used by reply and reject commands.
  // 回复和拒绝命令使用的稳定请求标识。
  id: string;
  // Controller session that owns the blocked ask tool call.
  // 拥有阻塞 ask 工具调用的控制器会话。
  sessionId: string;
  // Runtime sequence used to order concurrent ask requests.
  // 用于排序并发 ask 请求的运行时序号。
  sequence: number;
  // Ordered prompts shown in the ask modal.
  // ask 弹窗中按顺序展示的问题提示。
  questions: PendingQuestionPromptSummary[];
}

// PendingQuestionPromptSummary describes one prompt inside a pending ask request.
// PendingQuestionPromptSummary 描述待处理 ask 请求中的单条问题提示。
export interface PendingQuestionPromptSummary {
  // Full question text shown to the user.
  // 展示给用户的完整问题文本。
  question: string;
  // Short field header shown above the question.
  // 展示在问题上方的简短字段标题。
  header: string;
  // Selectable choices offered by this prompt.
  // 该问题提供的可选答案。
  options: PendingQuestionOptionSummary[];
  // Whether multiple structured choices may be selected together.
  // 是否允许同时选择多个结构化选项。
  multiple: boolean;
}

// PendingQuestionOptionSummary describes one selectable ask choice.
// PendingQuestionOptionSummary 描述 ask 问题中的单个可选答案。
export interface PendingQuestionOptionSummary {
  // Choice label returned to the model when selected.
  // 被选中时回传给模型的选项标签。
  label: string;
  // Helper text explaining the choice.
  // 解释该选项的辅助说明。
  description: string;
}

// SessionTodoPlanSummary describes the current model-authored session TODO plan.
// SessionTodoPlanSummary 描述当前模型编写的会话 TODO 计划。
export interface SessionTodoPlanSummary {
  // Optional explanation attached to the latest plan update.
  // 最近一次计划更新附带的可选说明。
  explanation?: string | null;
  // Ordered TODO items shown in the session tool panel.
  // 会话工具面板中展示的有序 TODO 条目。
  plan: SessionTodoPlanItemSummary[];
  // Completed item count.
  // 已完成条目数量。
  completedCount: number;
  // Total item count.
  // 条目总数量。
  totalCount: number;
  // Unix-millis update timestamp text.
  // Unix 毫秒格式的更新时间文本。
  updatedAt?: string | null;
}

// SessionTodoPlanItemSummary describes one TODO item for WebUI rendering.
// SessionTodoPlanItemSummary 描述供 WebUI 渲染的单个 TODO 条目。
export interface SessionTodoPlanItemSummary {
  // Human-readable step text supplied by the model.
  // 模型提供的人类可读步骤文本。
  step: string;
  // Stable status key: pending, in_progress, or completed.
  // 稳定状态键：pending、in_progress 或 completed。
  status: "pending" | "in_progress" | "completed";
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
  // Archived project summaries available for restore surfaces.
  // 可用于恢复界面的归档项目摘要。
  archivedProjects?: ProjectSummary[];
  // Archived conversation summaries available for restore surfaces.
  // 可用于恢复界面的归档会话摘要。
  archivedConversations?: ConversationSummary[];
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId?: string | null;
  // Conversation messages.
  // 会话消息。
  messages: ConversationMessage[];
  // Backend-routed subagent navigation state for the selected parent conversation.
  // 当前选中父会话的后端路由子代理导航状态。
  subagentNavigation?: SubagentNavigationSnapshot | null;
  // Tool call summaries.
  // 工具调用摘要。
  toolCalls: ToolCallSummary[];
  // Main feature areas.
  // 主功能区域。
  featureAreas: FeatureAreaSummary[];
  // Lua Skills catalog.
  // Lua Skills 目录。
  luaSkills: LuaSkillsCatalogSummary;
  // Agent Skills catalog.
  // Agent Skills 目录。
  agentSkills: AgentSkillsCatalogSummary;
  // Settings sections.
  // 设置分组。
  settings: SettingSectionSummary[];
  // Provider summaries.
  // Provider 摘要。
  providers: ProviderSummary[];
  // MCP server summaries.
  // MCP 服务器摘要。
  mcpServers?: McpServerSummary[];
  // Backend-resolved reasoning selector view for the active model.
  // 后端根据当前激活模型解析出的思考选择器视图。
  reasoningSelection?: ReasoningSelectionSummary | null;
  // AI capability route summaries for the composer quick controls.
  // 输入区快捷控制使用的 AI 能力路由摘要。
  aiCapabilities?: AiCapabilitySummary[];
  // Default AI capability route summaries used to reset one session draft.
  // 用于重置单个会话草稿的默认 AI 能力路由摘要。
  defaultAiCapabilities?: AiCapabilitySummary[];
  // Shared AI capability vendor summaries.
  // 共享 AI 能力供应商摘要。
  aiCapabilityVendors?: AiCapabilityVendorSummary[];
  // Shared AI capability vendor catalog choices.
  // 共享 AI 能力供应商目录选项。
  aiCapabilityVendorCatalog?: AiCapabilityVendorCatalogSummary[];
  // Attachment capability gates for the composer attachment controls.
  // 输入区附件控件使用的附件能力门控。
  composerAttachmentCapabilities?: ComposerAttachmentCapabilitySummary[];
  // Composer slash commands projected by the shared service command catalog.
  // 共享服务指令目录投影出的输入区 slash 指令。
  composerSlashCommands?: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts?: PendingPromptSummary[];
  // Oldest pending ask-tool request waiting for a user reply.
  // 当前等待用户回复的最早 ask 工具请求。
  pendingQuestion?: PendingQuestionSnapshot | null;
  // Current session TODO plan shown in the conversation tool panel.
  // 当前会话工具面板展示的会话 TODO 计划。
  sessionTodoPlan?: SessionTodoPlanSummary | null;
  // VMM memory summaries.
  // VMM 记忆摘要。
  memories: MemorySummary[];
  // Detailed VMM memory management snapshot.
  // 详细 VMM 记忆管理快照。
  vmmMemory: VmmMemorySnapshot;
  // Command session summaries.
  // 命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Diagnostic log entries.
  // 诊断日志条目。
  logs: LogEntrySummary[];
  // Context injection cards projected from the selected session runtime state.
  // 从当前选中会话运行态投影出的上下文注入卡片。
  contextInjection?: ContextInjectionSnapshot | null;
  // Context token usage snapshot projected from the latest execution preparation.
  // 从最近一次执行准备投影出的上下文 Token 占用快照。
  contextUsage?: ContextUsageSnapshot | null;
}
