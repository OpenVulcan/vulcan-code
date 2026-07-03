import type {
  AiCapabilitySummary,
  AiCapabilityVendorCatalogSummary,
  AiCapabilityVendorSummary,
} from "./ai-capabilities";
import type {
  AgentSkillsCatalogSummary,
  ComposerAttachmentCapabilitySummary,
  LuaSkillsCatalogSummary,
  PendingPromptSummary,
  SubagentCatalogSummary,
  ToolCallSummary,
} from "./composer-skills";
import type {
  ConversationMessage,
  ConversationSummary,
  ContextInjectionChangedDelta,
  PendingPromptsChangedDelta,
  SessionAddonsChangedDelta,
  SessionVmmPolicyByConversation,
  SessionVmmPolicySummary,
} from "./conversation";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type {
  MemorySummary,
  VmmMemorySnapshot,
} from "./memory-vmm";
import type {
  PendingQuestionSnapshot,
  ComposerSlashCommandSummary,
  SessionGoalSummary,
  SessionPlanModeSummary,
  SessionTodolistSummary,
  SubagentNavigationSnapshot,
} from "./planning-questions";
import type {
  ProjectSummary,
} from "./projects-files";
import type {
  ContextInjectionSnapshot,
  ContextUsageSnapshot,
  CommandSessionSummary,
  LogEntrySummary,
} from "./runtime-context";
import type {
  FeatureAreaSummary,
  HostToolsSettingsSummary,
  McpServerSummary,
  McpToolSummary,
  ProviderSummary,
  ReasoningSelectionSummary,
  SettingSectionSummary,
} from "./settings-providers";
import type { RuntimeModuleSnapshot } from "./startup";

export interface WorkbenchSnapshot {
  // Monotonic workbench revision shared by full snapshots and delta events.
  // 全量快照与增量事件共享的工作台单调版本号。
  revision?: number;
  // Application version.
  // 应用版本。
  appVersion: string;
  // WebUI locale resolved from the shared service configuration.
  // 从共享服务配置解析出的 WebUI 语言。
  uiLocale?: string | null;
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
  // Hydrated conversation body windows retained by the service.
  // 服务端已保留的会话正文窗口。
  hydratedConversationWindows?: HydratedConversationWindowSummary[];
  // Current lazy conversation body hydration progress.
  // 当前懒加载会话正文水合进度。
  conversationHydration?: ConversationHydrationProgress | null;
  // Backend-routed subagent navigation state for the selected parent conversation.
  // 当前选中父会话的后端路由子代理导航状态。
  subagentNavigation?: SubagentNavigationSnapshot | null;
  // Subagent navigation snapshots retained by their parent conversation ids.
  // 按父会话标识保留的子代理导航快照。
  subagentNavigationByConversation?: SubagentNavigationSnapshot[];
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
  // Subagent and expert catalog.
  // 子代理与专家目录。
  subagentCatalog: SubagentCatalogSummary;
  // Settings sections.
  // 设置分组。
  settings: SettingSectionSummary[];
  // Provider summaries.
  // Provider 摘要。
  providers: ProviderSummary[];
  // MCP server summaries.
  // MCP 服务器摘要。
  mcpServers?: McpServerSummary[];
  // MCP runtime tool summaries available for capability policy pickers.
  // 能力策略选择器可用的 MCP 运行时工具摘要。
  mcpTools?: McpToolSummary[];
  // Backend-resolved reasoning selector view for the active model.
  // 后端根据当前激活模型解析出的思考选择器视图。
  reasoningSelection?: ReasoningSelectionSummary | null;
  // Thinking-output display mode resolved from the shared service configuration.
  // 从共享服务配置解析出的思考输出展示模式。
  thinkingDisplayMode?: ThinkingDisplayMode | null;
  // Git commit dialog settings resolved from the shared service configuration.
  // 从共享服务配置解析出的 Git 提交弹窗设置。
  gitCommit?: GitCommitSettingsSummary;
  // Host-owned tool settings resolved from the shared service configuration.
  // 从共享服务配置解析出的宿主自有工具设置。
  hostTools?: HostToolsSettingsSummary;
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
  // Pending prompt queues retained for every conversation that has one.
  // 为每个存在待发送队列的会话保留的待发送提示词队列。
  pendingPromptsByConversation?: PendingPromptsChangedDelta[];
  // Oldest pending ask-tool request waiting for a user reply.
  // 当前等待用户回复的最早 ask 工具请求。
  pendingQuestion?: PendingQuestionSnapshot | null;
  // Pending ask-tool questions grouped by their owning conversation.
  // 按所属会话保留的 ask 工具待回答问题。
  pendingQuestions?: PendingQuestionSnapshot[];
  // Current session todolist shown in the conversation tool panel.
  // 当前会话工具面板展示的会话 todolist。
  sessionTodolist?: SessionTodolistSummary | null;
  // Current session goal shown above the conversation composer.
  // 当前会话输入区上方展示的会话目标。
  sessionGoal?: SessionGoalSummary | null;
  // Current session Plan mode state shown in the composer controls.
  // 当前会话输入区控制项展示的计划模式状态。
  sessionPlanMode?: SessionPlanModeSummary | null;
  // Current session VMM memory policy shown in the composer controls.
  // 当前会话输入区控制项展示的 VMM 记忆策略。
  sessionVmmPolicy?: SessionVmmPolicySummary | null;
  // Session addon projections retained for every conversation that has addon state.
  // 为每个存在附属状态的会话保留的会话附属投影。
  sessionAddonsByConversation?: SessionAddonsChangedDelta[];
  // VMM policy projections retained for every visible conversation.
  // 为每个可见会话保留的 VMM 策略投影。
  sessionVmmPoliciesByConversation?: SessionVmmPolicyByConversation[];
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
  // Context injection projections retained by conversation id.
  // 按会话标识保留的上下文注入投影。
  contextInjectionByConversation?: ContextInjectionChangedDelta[];
  // Context token usage snapshot projected from the latest execution preparation.
  // 从最近一次执行准备投影出的上下文 Token 占用快照。
  contextUsage?: ContextUsageSnapshot | null;
}

// GitCommitSettingsSummary describes Git commit dialog settings projected by the service.
// GitCommitSettingsSummary 描述服务端投影出的 Git 提交弹窗设置。
export interface GitCommitSettingsSummary {
  // Default namespace prefix used when the current branch has no namespace.
  // 当前分支没有命名空间时使用的默认命名空间前缀。
  newBranchPrefix: string;
  // Model selection used when commit messages are generated automatically.
  // 自动生成提交信息时使用的模型选择。
  summaryModel: GitCommitSummaryModelSelectionSummary;
  // User-authored extra prompt appended to commit-message generation.
  // 追加到提交信息生成中的用户额外提示词。
  summaryExtraPrompt: string;
  // Whether commit-message generation should prefer RTK-compressed Git input.
  // 提交信息生成是否优先使用 RTK 压缩后的 Git 输入。
  useRtkSummary: boolean;
}

// GitCommitSummaryModelSelectionSummary describes the automatic commit-summary model setting.
// GitCommitSummaryModelSelectionSummary 描述自动提交总结模型设置。
export type GitCommitSummaryModelSelectionSummary =
  | {
    // Selection mode that follows the owning conversation session.
    // 跟随所属会话的选择模式。
    mode: "session";
  }
  | {
    // Selection mode that uses a fixed provider/model pair.
    // 使用固定 Provider/模型组合的选择模式。
    mode: "fixed";
    // Stable provider identifier.
    // 稳定 Provider 标识。
    providerId: string;
    // Stable model identifier.
    // 稳定模型标识。
    modelId: string;
  };

// HydratedConversationWindowSummary describes one retained lazy conversation body window.
// HydratedConversationWindowSummary 描述一个已保留的懒加载会话正文窗口。
export interface HydratedConversationWindowSummary {
  // Conversation identifier owning this retained body window.
  // 拥有该保留正文窗口的会话标识。
  sessionId: string;
  // Earliest loaded transcript ordinal used to request older history.
  // 用于继续请求更早历史的已加载最早转录序号。
  earliestOrdinal?: number | null;
  // Whether older transcript turns still exist before this window.
  // 当前窗口之前是否仍存在更早的转录轮次。
  hasMoreHistory: boolean;
}

// ConversationHydrationStatus describes lazy conversation body loading state.
// ConversationHydrationStatus 描述懒加载会话正文的加载状态。
export type ConversationHydrationStatus = "loading" | "ready" | "error";

// ConversationHydrationProgress describes one lazy conversation body loading update.
// ConversationHydrationProgress 描述单个懒加载会话正文的加载进度更新。
export interface ConversationHydrationProgress {
  // Conversation identifier being loaded.
  // 正在加载的会话标识。
  sessionId: string;
  // Current loading status.
  // 当前加载状态。
  status: ConversationHydrationStatus;
  // Current phase key used for localization.
  // 用于本地化的当前阶段键。
  phase: string;
  // Human-readable fallback message.
  // 面向用户的回退消息。
  message: string;
  // Completed progress percentage from 0 to 100.
  // 0 到 100 的已完成百分比。
  percent: number;
}

// ConversationMessagesLoadResponse contains one lazily loaded conversation body.
// ConversationMessagesLoadResponse 包含单个懒加载完成的会话正文。
export interface ConversationMessagesLoadResponse {
  // Complete workbench snapshot rebuilt with the hydrated selected conversation.
  // 使用已水合选中会话重建出的完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Loaded conversation messages.
  // 已加载的会话消息。
  messages: ConversationMessage[];
  // Earliest loaded transcript ordinal used to request older history.
  // 用于继续请求更早历史的已加载最早转录序号。
  earliestOrdinal?: number | null;
  // Whether older transcript turns still exist before this window.
  // 当前窗口之前是否仍存在更早的转录轮次。
  hasMoreHistory: boolean;
  // Final loading progress.
  // 最终加载进度。
  progress: ConversationHydrationProgress;
}

// ConversationHistoryPageResponse contains one older conversation history page.
// ConversationHistoryPageResponse 包含单页更早会话历史。
export interface ConversationHistoryPageResponse {
  // Conversation identifier owning this page.
  // 拥有该页历史的会话标识。
  sessionId: string;
  // Complete workbench snapshot after merging this older page.
  // 合并该页更早历史后的完整工作台快照。
  workbench: WorkbenchSnapshot;
  // Older conversation messages in ascending display order.
  // 按展示正序排列的更早会话消息。
  messages: ConversationMessage[];
  // Earliest loaded transcript ordinal after this page.
  // 加载该页后已加载的最早转录序号。
  earliestOrdinal?: number | null;
  // Whether more older transcript turns still exist.
  // 是否仍存在更多更早的转录轮次。
  hasMoreHistory: boolean;
}
