import type {
  AgentSkillsCatalogSummary,
  ComposerAttachmentSummary,
  LuaSkillsCatalogSummary,
  LuaSkillsManagedRuntimeProgressSummary,
  PendingPromptSummary,
  PluginSummary,
  SubagentCatalogSummary,
} from "./composer-skills";
import type {
  PendingQuestionSnapshot,
  SessionGoalSummary,
  SessionPlanModeSummary,
  SessionTodolistSummary,
  SubagentNavigationSnapshot,
} from "./planning-questions";
import type { CommandSessionSummary, ContextInjectionSnapshot, ContextUsageSnapshot } from "./runtime-context";
import type { ProjectSummary } from "./projects-files";
import type { ConversationHydrationProgress } from "./workbench";

// ConversationRunOutcome identifies the strict terminal result of a settled run.
// ConversationRunOutcome 标识已结束运行的严格终态结果。
export type ConversationRunOutcome = "completed" | "failed";

// ConversationRunSettledEvent describes one platform-neutral final run event.
// ConversationRunSettledEvent 描述一个与平台无关的运行最终结束事件。
export interface ConversationRunSettledEvent {
  // eventId is the stable idempotency key for this terminal event.
  // eventId 是本终态事件的稳定幂等键。
  eventId: string;
  // conversationId identifies the conversation that owned the run.
  // conversationId 标识拥有本次运行的会话。
  conversationId: string;
  // projectId identifies the authoritative owning project when one exists.
  // projectId 标识存在时的权威所属项目。
  projectId?: string | null;
  // runId is the exact stream task identity that owned the terminal report.
  // runId 是持有终态报告的精确流任务身份。
  runId: string;
  // outcome is the strict terminal result.
  // outcome 是严格终态结果。
  outcome: ConversationRunOutcome;
  // occurredAt is the Unix-millisecond timestamp text captured by the service.
  // occurredAt 是服务端捕获的 Unix 毫秒时间戳文本。
  occurredAt: string;
}

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
  // Highest-priority action marker that belongs to this conversation.
  // 属于该会话的最高优先级待处理动作标记。
  attention?: ConversationAttentionSummary | null;
  // Optional owning project identifier.
  // 可选所属项目标识。
  projectId?: string | null;
  // Display-only fork source marker.
  // 纯展示用分叉来源标记。
  forkSource?: ConversationForkSourceSummary | null;
  // Revisioned prompt-free continuation admission state.
  // 带修订的无提示词 Continue 准入状态。
  continuation?: ConversationContinuationSummary | null;
}

// ConversationContinuationSummary is the exact backend claim token for Continue.
// ConversationContinuationSummary 是 Continue 使用的精确后端领取令牌。
export interface ConversationContinuationSummary {
  // Whether the canonical terminal boundary may continue without a new prompt.
  // 规范终止边界是否允许在没有新提示词时继续。
  eligible: boolean;
  // Exact continuation generation.
  // 精确 Continue 代际。
  generation: number;
  // Exact session Provider ledger revision.
  // 精确会话 Provider 账本修订号。
  expectedLedgerRevision: number;
  // Stable reason recorded by the terminal continuation boundary.
  // 终止 Continue 边界记录的稳定原因。
  reason?: string | null;
}

// ConversationAttentionSummary describes one conversation-level action marker.
// ConversationAttentionSummary 描述单个会话级动作标记。
export interface ConversationAttentionSummary {
  // Stable marker kind such as ask, plan_confirm, or unread.
  // 稳定标记类型，例如 ask、plan_confirm 或 unread。
  kind: "ask" | "plan_confirm" | "tool_confirm" | "subagent_attention" | "unread" | string;
  // Numeric priority used by clients when multiple markers exist.
  // 多个标记同时存在时客户端使用的数字优先级。
  priority: number;
  // Optional request or artifact identifier that owns this marker.
  // 拥有该标记的可选请求或产物标识。
  requestId?: string | null;
  // Optional human-readable label for tooltips and assistive text.
  // 用于提示与辅助文本的可选人类可读标签。
  label?: string | null;
  // Unix-millisecond timestamp text for ordering diagnostics.
  // 用于排序诊断的 Unix 毫秒时间戳文本。
  updatedAt?: string | null;
}

// ConversationForkSourceSummary describes where a forked conversation was derived from.
// ConversationForkSourceSummary 描述分叉会话从哪里派生。
export interface ConversationForkSourceSummary {
  // Source project tag used to locate the original conversation.
  // 用于定位原始会话的源项目标签。
  sourceProjectTag: string;
  // Source conversation identifier for navigation.
  // 用于导航的源会话标识。
  sourceConversationId: string;
  // Source conversation title captured at fork time.
  // 分叉时捕获的源会话标题。
  sourceTitle: string;
  // Last retained message id at the fork boundary.
  // 分叉边界处最后保留的消息标识。
  anchorMessageId?: string | null;
  // Fork creation timestamp.
  // 分叉创建时间戳。
  createdAt: string;
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
  // Whether the backend allows editing and replaying this user prompt.
  // 后端是否允许编辑并重新执行该用户提示词。
  editableUserPrompt?: boolean;
}

// ConversationMessageStreamState describes whether a streamed message can still grow.
// ConversationMessageStreamState 描述流式消息是否仍可继续增长。
export type ConversationMessageStreamState = "active" | "sealed";

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
  // Explicit lifecycle for a streamed thinking or assistant-output message.
  // 流式思考或助手输出消息的显式生命周期。
  streamState?: ConversationMessageStreamState | null;
}

// TransientReasoningStatus is one replace-in-place status excluded from transcript persistence.
// TransientReasoningStatus 是一条排除在转录持久化之外的原地替换式状态。
export interface TransientReasoningStatus {
  // Presentation kind selected by the response profile.
  // 由响应 Profile 选择的展示类型。
  kind: "summary" | "encrypted";
  // Latest readable provider summary when available.
  // 可用时的最新供应商可读摘要。
  text?: string | null;
  // Latest cumulative reasoning-token count when available.
  // 可用时的最新累计推理 Token 数。
  reasoningTokens?: number | null;
}

// TransientReasoningStatusChangedDelta replaces or clears one conversation status line.
// TransientReasoningStatusChangedDelta 替换或清除单个会话状态行。
export interface TransientReasoningStatusChangedDelta {
  // Stable conversation identifier owning the active request.
  // 拥有当前活动请求的稳定会话标识。
  conversationId: string;
  // Latest status, or null to clear the line.
  // 最新状态；为 null 时清除该状态行。
  status?: TransientReasoningStatus | null;
}

// UpdateConversationRunPhaseDelta updates a conversation phase without transcript rows.
// UpdateConversationRunPhaseDelta 在不携带转录行的情况下更新会话阶段。
export interface UpdateConversationRunPhaseDelta {
  // Stable conversation identifier receiving the phase update.
  // 接收阶段更新的稳定会话标识。
  conversationId: string;
  // Current backend run phase for the conversation.
  // 会话当前的后端运行阶段。
  runPhase: string;
  // Optional backend status label paired with the phase.
  // 与阶段配套的可选后端状态标签。
  status?: string | null;
  // Optional relative update label for the conversation row.
  // 会话行使用的可选相对更新时间标签。
  lastUpdatedLabel?: string | null;
}

// PendingPromptsChangedDelta replaces the visible pending prompt queue for one conversation.
// PendingPromptsChangedDelta 替换单个会话可见的待发送提示词队列。
export interface PendingPromptsChangedDelta {
  // Stable conversation identifier receiving the queue projection.
  // 接收队列投影的稳定会话标识。
  conversationId: string;
  // Current visible pending prompt summaries for the conversation.
  // 当前会话可见的待发送提示词摘要。
  pendingPrompts: PendingPromptSummary[];
}

// ConversationSummaryChangedDelta upserts one sidebar conversation summary.
// ConversationSummaryChangedDelta 写入单个侧栏会话摘要。
export interface ConversationSummaryChangedDelta {
  // Current conversation summary projected for the sidebar.
  // 当前投影到侧栏的会话摘要。
  conversation: ConversationSummary;
  // Selected conversation id after applying this summary update.
  // 应用该摘要更新后的选中会话标识。
  selectedConversationId?: string | null;
}

// ConversationArchivedDelta applies one committed sidebar lifecycle transition.
// ConversationArchivedDelta 应用单个已提交的侧栏生命周期转换。
export interface ConversationArchivedDelta {
  // Stable identifier removed from the active conversation collection.
  // 从活动会话集合移除的稳定会话标识。
  conversationId: string;
  // Authoritative archived summary when the session remains restorable.
  // 当会话仍可恢复时对应的权威归档摘要。
  archivedConversation?: ConversationSummary | null;
  // Selected active conversation identifier after the archive transition.
  // 归档转换完成后的选中活动会话标识。
  selectedConversationId?: string | null;
}

// ConversationsArchivedDelta carries every conversation archive committed by one project action.
// ConversationsArchivedDelta 携带单次项目操作提交的全部会话归档结果。
export interface ConversationsArchivedDelta {
  // Exact conversation lifecycle transitions committed in the shared generation.
  // 在同一 generation 中提交的精确会话生命周期转换。
  archivedConversations: ConversationArchivedDelta[];
}

// ProjectRemovedDelta carries one committed project removal and its child archive transitions.
// ProjectRemovedDelta 携带单次已提交项目移除及其子会话归档转换。
export interface ProjectRemovedDelta {
  // Exact active project identifier removed by the lifecycle transaction.
  // 生命周期事务移除的精确活动项目标识。
  projectId: string;
  // Authoritative active projects after removal and default-project repair.
  // 移除及默认项目修复后的权威活动项目集合。
  projects: ProjectSummary[];
  // Authoritative expanded project identifiers after removal.
  // 移除后的权威展开项目标识集合。
  expandedProjectIds: string[];
  // Exact child conversation archive transitions committed with project removal.
  // 与项目移除一同提交的精确子会话归档转换。
  archivedConversations: ConversationArchivedDelta[];
}

// PendingQuestionChangedDelta replaces the active ask-tool pending question.
// PendingQuestionChangedDelta 替换当前 ask 工具待回答问题。
export interface PendingQuestionChangedDelta {
  // Stable conversation identifier receiving the ask projection.
  // 接收 ask 投影的稳定会话标识。
  conversationId?: string | null;
  // Current pending question, or null when no ask question is waiting.
  // 当前待回答问题；没有等待中的 ask 问题时为空。
  pendingQuestion?: PendingQuestionSnapshot | null;
  // All pending ask-tool questions grouped by their own session ids.
  // 按各自会话标识保留的全部 ask 工具待回答问题。
  pendingQuestions?: PendingQuestionSnapshot[];
}

// CommandSessionsChangedDelta replaces the workbench command-session summaries.
// CommandSessionsChangedDelta 替换工作台命令会话摘要。
export interface CommandSessionsChangedDelta {
  // Current command-session summaries projected from the live runtime.
  // 从实时运行时投影出的当前命令会话摘要。
  commandSessions: CommandSessionSummary[];
}

// SubagentNavigationChangedDelta replaces the floating/side-panel subagent navigation state.
// SubagentNavigationChangedDelta 替换浮动面板与右侧页签使用的子代理导航状态。
export interface SubagentNavigationChangedDelta {
  // Parent conversation identifier receiving this navigation projection.
  // 接收该导航投影的父会话标识。
  parentConversationId?: string | null;
  // Current subagent navigation, or null when no subagent panel should be shown.
  // 当前子代理导航；不应展示子代理面板时为空。
  subagentNavigation?: SubagentNavigationSnapshot | null;
}

// SubagentPanelRouteResponse carries the focused route state after a subagent panel action.
// SubagentPanelRouteResponse 携带子代理面板动作后的聚焦路由状态。
export interface SubagentPanelRouteResponse {
  // Parent conversation id that owns this route response.
  // 拥有本次路由响应的父会话标识。
  parentConversationId?: string | null;
  // Conversation id that owns the returned message window.
  // 拥有返回消息窗口的会话标识。
  activeConversationId?: string | null;
  // Current subagent navigation, or null when no subagent panel is active.
  // 当前子代理导航；没有活动子代理面板时为空。
  subagentNavigation?: SubagentNavigationSnapshot | null;
  // Messages for the active surface only.
  // 仅当前活动界面的消息列表。
  messages: ConversationMessage[];
  // Latest workbench revision after applying the route update.
  // 应用路由更新后的最新工作台版本。
  revision: number;
}

// ContextUsageChangedDelta replaces the composer context-usage projection.
// ContextUsageChangedDelta 替换输入区上下文用量投影。
export interface ContextUsageChangedDelta {
  // Current context usage, or null when no active usage is available.
  // 当前上下文用量；没有活动用量时为空。
  contextUsage?: ContextUsageSnapshot | null;
}

// ContextInjectionChangedDelta replaces the selected-session memory-panel projection.
// ContextInjectionChangedDelta 替换当前选中会话的记忆面板投影。
export interface ContextInjectionChangedDelta {
  // Stable conversation identifier that owns this memory-panel projection.
  // 拥有该记忆面板投影的稳定会话标识。
  conversationId: string;
  // Current context injection projection, or null when runtime state is unavailable.
  // 当前上下文注入投影；运行态不可用时为空。
  contextInjection?: ContextInjectionSnapshot | null;
}

// ConversationMessagesChangedDelta replaces one focused conversation message window.
// ConversationMessagesChangedDelta 替换单个聚焦会话的消息窗口。
export interface ConversationMessagesChangedDelta {
  // Stable conversation identifier receiving the message window.
  // 接收消息窗口的稳定会话标识。
  conversationId: string;
  // Current visible messages for the conversation.
  // 当前会话的可见消息列表。
  messages: ConversationMessage[];
}

// ConversationMessagesPatchedDelta patches visible message nodes without replacing the whole window.
// ConversationMessagesPatchedDelta 在不替换整个窗口的情况下修补可见消息节点。
export interface ConversationMessagesPatchedDelta {
  // Stable conversation identifier receiving the message patch.
  // 接收消息补丁的稳定会话标识。
  conversationId: string;
  // Full message nodes to insert or replace by id.
  // 需要按标识插入或替换的完整消息节点。
  upsertMessages: ConversationMessage[];
  // Message ids that should be removed from the visible window.
  // 需要从可见窗口移除的消息标识。
  removeMessageIds: string[];
  // Optional authoritative order for visible nodes in this conversation.
  // 当前会话可见节点的可选权威顺序。
  messageOrder?: string[] | null;
  // Optional run phase that should be applied atomically with this message patch.
  // 需要与本次消息补丁原子应用的可选运行阶段。
  runPhase?: string | null;
}

// SessionAddonsChangedDelta replaces goal, todolist, and Plan mode projections.
// SessionAddonsChangedDelta 替换目标、待办和计划模式投影。
export interface SessionAddonsChangedDelta {
  // Stable conversation identifier receiving the addon projection.
  // 接收附属状态投影的稳定会话标识。
  conversationId: string;
  // Current session goal, or null when no goal is active.
  // 当前会话目标；没有活动目标时为空。
  sessionGoal?: SessionGoalSummary | null;
  // Current session todolist, or null when no todolist is visible.
  // 当前会话待办；没有可见待办时为空。
  sessionTodolist?: SessionTodolistSummary | null;
  // Current Plan mode state, or null when Plan mode is inactive and no artifact exists.
  // 当前计划模式状态；计划模式未启用且无产物时为空。
  sessionPlanMode?: SessionPlanModeSummary | null;
  // Current session VMM memory policy overrides.
  // 当前会话 VMM 记忆策略覆盖。
  sessionVmmPolicy?: SessionVmmPolicySummary | null;
  // Current output-style override and the last safely applied snapshot.
  // 当前会话输出风格覆盖与最近一次安全应用的快照。
  sessionOutputStyle?: SessionOutputStyleSummary | null;
  // Live physical Provider request counts and token usage.
  // 实时物理 Provider 请求计数与 Token 用量。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
}

// SessionOutputStyleSummary describes a pending per-session style override without mutating the active prompt context.
// SessionOutputStyleSummary 描述待生效的会话风格覆盖，不直接修改当前提示词上下文。
export interface SessionOutputStyleSummary {
  // Explicit per-session override, or null when inheriting the global setting.
  // 会话显式覆盖；继承全局设置时为空。
  overrideStyleId?: string | null;
  // Desired style after resolving the session override against the global setting.
  // 将会话覆盖与全局设置解析后的目标风格。
  desiredStyleId: string;
  // Last style snapshot safely applied at a prompt boundary.
  // 最近一次在提示词边界安全应用的风格快照。
  appliedStyleId?: string | null;
  // Whether the desired style waits for a new session or compaction boundary.
  // 目标风格是否等待新会话或压缩边界。
  pendingActivation: boolean;
  // Fingerprint of the applied prompt text when available.
  // 已应用提示词正文的指纹（如可用）。
  appliedFingerprint?: number | null;
  // Timestamp of the last safe application.
  // 最近一次安全应用的时间戳。
  appliedAt?: string | null;
  // Next lifecycle boundary that can activate a pending selection.
  // 可激活待生效选择的下一个生命周期边界。
  nextActivationBoundary?: string | null;
}

// SessionVmmPolicySummary describes session VMM policy override and effective state.
// SessionVmmPolicySummary 描述会话 VMM 策略覆盖与最终状态。
export interface SessionVmmPolicySummary {
  // Precheck memory injection policy mode: inherit, enabled, or disabled.
  // 前置记忆注入策略模式：跟随、开启或关闭。
  precheckMode: string;
  // Effective precheck memory injection enablement.
  // 最终前置记忆注入启用状态。
  precheckEnabled: boolean;
  // Completed-turn memory writeback policy mode: inherit, enabled, or disabled.
  // 完成回合记忆写回策略模式：跟随、开启或关闭。
  memoryWritebackMode: string;
  // Effective completed-turn memory writeback enablement.
  // 最终完成回合记忆写回启用状态。
  memoryWritebackEnabled: boolean;
}

// SessionVmmPolicyByConversation stores one conversation's VMM policy projection.
// SessionVmmPolicyByConversation 保存单个会话的 VMM 策略投影。
export interface SessionVmmPolicyByConversation {
  // Stable conversation id that owns the VMM policy projection.
  // 拥有该 VMM 策略投影的稳定会话标识。
  conversationId: string;
  // VMM policy override and effective state for this conversation.
  // 该会话的 VMM 策略覆盖与最终状态。
  sessionVmmPolicy: SessionVmmPolicySummary;
}

// SessionProviderUsageTokenSummary carries one live Provider token snapshot.
// SessionProviderUsageTokenSummary 携带一份实时 Provider Token 快照。
export interface SessionProviderUsageTokenSummary {
  // Provider-reported input tokens when known.
  // Provider 报告的输入 Token，未知时为空。
  inputTokens?: number | null;
  // Normalized uncached input tokens when known.
  // 规范化的非缓存输入 Token，未知时为空。
  uncachedInputTokens?: number | null;
  // Provider-reported cached input tokens when known.
  // Provider 报告的缓存命中输入 Token，未知时为空。
  cachedInputTokens?: number | null;
  // Provider-reported cache-creation input tokens when known.
  // Provider 报告的缓存写入输入 Token，未知时为空。
  cacheCreationInputTokens?: number | null;
  // Provider-reported output tokens when known.
  // Provider 报告的输出 Token，未知时为空。
  outputTokens?: number | null;
  // Provider-reported reasoning tokens when known.
  // Provider 报告的推理 Token，未知时为空。
  reasoningTokens?: number | null;
  // Provider-reported total tokens when known.
  // Provider 报告的总 Token，未知时为空。
  totalTokens?: number | null;
}

// SessionProviderUsageCallSummary carries the most recent physical request lifecycle.
// SessionProviderUsageCallSummary 携带最近一次物理请求生命周期。
export interface SessionProviderUsageCallSummary {
  // Stable physical Provider request identifier.
  // 稳定的物理 Provider 请求标识。
  providerCallId: string;
  // Stable execution owner category.
  // 稳定的执行归属类别。
  ownerKind: string;
  // Request lifecycle status.
  // 请求生命周期状态。
  status: string;
  // Usage facts known for this request.
  // 此请求已知的用量事实。
  usage?: SessionProviderUsageTokenSummary | null;
  // Provider dispatch timestamp in Unix milliseconds.
  // Provider 分发时间的 Unix 毫秒时间戳。
  startedAtMs?: number | null;
  // Provider terminal timestamp in Unix milliseconds.
  // Provider 终态时间的 Unix 毫秒时间戳。
  completedAtMs?: number | null;
  // Stable redacted error category.
  // 稳定的脱敏错误类别。
  errorCategory?: string | null;
  // Stable redacted error code.
  // 稳定的脱敏错误代码。
  errorCode?: string | null;
}

// SessionProviderUsageAggregateSummary preserves one exact branch or billing scope.
// SessionProviderUsageAggregateSummary 保留一个精确的分支或账单统计口径。
export interface SessionProviderUsageAggregateSummary {
  // Number of physical calls in this scope.
  // 当前口径内的物理调用数量。
  providerCallCount: number;
  // Number of terminal calls without usable Provider usage.
  // 当前口径内没有可用 Provider 用量的终态调用数量。
  unknownUsageCallCount: number;
  // Provider-reported cumulative usage in this scope.
  // 当前口径内 Provider 实测累计用量。
  usage?: SessionProviderUsageTokenSummary | null;
}

// SessionProviderUsageSummary carries live physical Provider accounting for one conversation.
// SessionProviderUsageSummary 携带单个会话的实时物理 Provider 统计。
export interface SessionProviderUsageSummary {
  // Number of physical Provider requests dispatched.
  // 已分发的物理 Provider 请求数量。
  providerCallCount: number;
  // Number of requests awaiting terminal events.
  // 等待终态事件的请求数量。
  inFlightCallCount: number;
  // Number of terminal requests without usable usage facts.
  // 未提供可用用量事实的终态请求数量。
  unknownUsageCallCount: number;
  // Latest usable usage snapshot.
  // 最近一份可用用量快照。
  lastUsage?: SessionProviderUsageTokenSummary | null;
  // Cumulative usable usage across terminal requests.
  // 全部终态请求的可用用量累计值。
  cumulativeUsage?: SessionProviderUsageTokenSummary | null;
  // Current active branch, including inherited context calls.
  // 当前活动分支，包含继承的上下文调用。
  activeBranch: SessionProviderUsageAggregateSummary;
  // Calls physically produced in this session, including superseded edit branches.
  // 此会话真实产生的调用，包含编辑后被取代的分支。
  lifetime: SessionProviderUsageAggregateSummary;
  // Calls inherited by a fork and not billed again in this session.
  // 分叉继承且未在当前会话重复计费的调用。
  inherited: SessionProviderUsageAggregateSummary;
  // Latest physical request lifecycle.
  // 最近一次物理请求生命周期。
  latestCall?: SessionProviderUsageCallSummary | null;
  // Monotonic usage revision.
  // 单调递增的用量版本号。
  usageRevision: number;
}

// SessionAddonsChangedResponse carries addon state returned by direct RPC calls.
// SessionAddonsChangedResponse 携带直接 RPC 返回的附属状态。
export interface SessionAddonsChangedResponse extends SessionAddonsChangedDelta {
  // Latest workbench revision after applying this addon update.
  // 应用该附属状态更新后的最新工作台版本。
  revision: number;
}

// SubmitConversationPromptAck acknowledges that a composer prompt was accepted by the service.
// SubmitConversationPromptAck 确认输入区提示词已被服务接收。
export interface SubmitConversationPromptAck {
  // Stable client-generated identifier used to reconcile the optimistic submission.
  // 用于协调乐观提交状态的客户端稳定生成标识。
  clientSubmissionId: string;
  // Stable conversation identifier that owns the accepted prompt.
  // 拥有已接收提示词的稳定会话标识。
  conversationId: string;
  // Visible user-message record id when one was materialized immediately.
  // 立即物化可见用户消息时对应的记录标识。
  acceptedMessageId?: string | null;
  // Runtime turn identifier used by the started kernel turn when available.
  // 已启动内核轮次可用时对应的运行轮次标识。
  turnId?: string | null;
  // Current run phase after accepting the prompt.
  // 接收提示词后的当前运行阶段。
  runPhase: string;
  // Whether the prompt was queued behind existing work.
  // 该提示词是否已排队等待既有工作结束。
  queued: boolean;
  // Latest accepted workbench revision after the submit-side update.
  // 提交侧更新后的最新已接收工作台版本。
  revision: number;
}

// WorkbenchDelta describes one partial workbench update emitted on hot paths.
// WorkbenchDelta 描述热路径上发出的单条局部工作台更新。
export type WorkbenchDelta =
  | { kind: "appendMessageBody"; payload: AppendMessageBodyDelta }
  | { kind: "updateConversationRunPhase"; payload: UpdateConversationRunPhaseDelta }
  | { kind: "pendingPromptsChanged"; payload: PendingPromptsChangedDelta }
  | { kind: "conversationSummaryChanged"; payload: ConversationSummaryChangedDelta }
  | { kind: "conversationArchived"; payload: ConversationArchivedDelta }
  | { kind: "conversationsArchived"; payload: ConversationsArchivedDelta }
  | { kind: "projectRemoved"; payload: ProjectRemovedDelta }
  | { kind: "pendingQuestionChanged"; payload: PendingQuestionChangedDelta }
  | { kind: "commandSessionsChanged"; payload: CommandSessionsChangedDelta }
  | { kind: "subagentNavigationChanged"; payload: SubagentNavigationChangedDelta }
  | { kind: "contextUsageChanged"; payload: ContextUsageChangedDelta }
  | { kind: "contextInjectionChanged"; payload: ContextInjectionChangedDelta }
  | { kind: "conversationMessagesChanged"; payload: ConversationMessagesChangedDelta }
  | { kind: "conversationMessagesPatched"; payload: ConversationMessagesPatchedDelta }
  | { kind: "sessionAddonsChanged"; payload: SessionAddonsChangedDelta }
  | { kind: "conversationHydrationChanged"; payload: ConversationHydrationChangedDelta }
  | { kind: "agentSkillsCatalogChanged"; payload: AgentSkillsCatalogChangedDelta }
  | { kind: "luaSkillsCatalogChanged"; payload: LuaSkillsCatalogChangedDelta }
  | { kind: "subagentCatalogChanged"; payload: SubagentCatalogChangedDelta }
  | { kind: "pluginCatalogChanged"; payload: PluginCatalogChangedDelta }
  | { kind: "projectSettingsChanged"; payload: ProjectSettingsChangedDelta }
  | {
    kind: "luaSkillsManagedRuntimeProgressChanged";
    payload: LuaSkillsManagedRuntimeProgressChangedDelta;
  }
  | { kind: "mediaUploadProgressChanged"; payload: MediaUploadProgressChangedDelta }
  | {
    kind: "transientReasoningStatusChanged";
    payload: TransientReasoningStatusChangedDelta;
  };

// AgentSkillsCatalogChangedDelta replaces only the Agent Skills workbench domain.
// AgentSkillsCatalogChangedDelta 仅替换工作台中的 Agent Skills 领域。
export interface AgentSkillsCatalogChangedDelta {
  // Complete Agent Skills catalog committed at the envelope generation.
  // 在信封版本提交的完整 Agent Skills 目录。
  agentSkills: AgentSkillsCatalogSummary;
}

// LuaSkillsCatalogChangedDelta replaces the LuaSkills catalog and matching plugin rows atomically.
// LuaSkillsCatalogChangedDelta 原子替换 LuaSkills 目录及其匹配的插件行。
export interface LuaSkillsCatalogChangedDelta {
  // Complete LuaSkills catalog committed at the envelope generation.
  // 在信封版本提交的完整 LuaSkills 目录。
  luaSkills: LuaSkillsCatalogSummary;
  // Plugin rows derived from the same committed catalog.
  // 从同一份已提交目录派生的插件行。
  plugins: PluginSummary[];
}

// SubagentCatalogChangedDelta replaces only the complete subagent catalog domain.
// SubagentCatalogChangedDelta 仅替换完整的智能体目录领域。
export interface SubagentCatalogChangedDelta {
  // Complete subagent catalog committed at the envelope generation.
  // 在信封版本提交的完整智能体目录。
  subagentCatalog: SubagentCatalogSummary;
}

// PluginCatalogChangedDelta replaces every plugin-owned catalog domain from one immutable snapshot.
// PluginCatalogChangedDelta 使用同一份不可变快照替换全部插件所有的目录领域。
export interface PluginCatalogChangedDelta {
  // Complete LuaSkills catalog captured during the same plugin transaction.
  // 在同一插件事务中捕获的完整 LuaSkills 目录。
  luaSkills: LuaSkillsCatalogSummary;
  // Plugin rows projected from the committed plugin catalog snapshot.
  // 从已提交插件目录快照投影出的插件行。
  plugins: PluginSummary[];
  // Complete Agent Skills catalog containing the same snapshot's plugin skills.
  // 包含同一快照插件 Skill 的完整 Agent Skills 目录。
  agentSkills: AgentSkillsCatalogSummary;
  // Complete subagent catalog containing the same snapshot's plugin agents.
  // 包含同一快照插件 Agent 的完整智能体目录。
  subagentCatalog: SubagentCatalogSummary;
}

// ProjectSettingsChangedDelta invalidates independently loaded settings for one exact project.
// ProjectSettingsChangedDelta 使单个准确项目独立加载的设置失效。
export interface ProjectSettingsChangedDelta {
  // Stable project identifier whose settings changed, or null when a global setting invalidates every project.
  // 设置发生变化的稳定项目标识；全局设置使所有项目失效时为 null。
  projectId: string | null;
  // Exact independently loaded settings domain that changed.
  // 发生变化的准确独立加载设置领域。
  domain: "vmm" | "agentSkills" | "codebaseMemory";
}

// LuaSkillsManagedRuntimeProgressChangedDelta replaces only the active managed-runtime progress.
// LuaSkillsManagedRuntimeProgressChangedDelta 仅替换当前受管运行时进度。
export interface LuaSkillsManagedRuntimeProgressChangedDelta {
  // Current native runtime installation progress, or null after the operation reaches a terminal state.
  // 当前原生运行时安装进度；操作到达终态后为 null。
  progress: LuaSkillsManagedRuntimeProgressSummary | null;
}

// MediaUploadProgressChangedDelta describes ephemeral bytes prepared before the prompt is sent.
// MediaUploadProgressChangedDelta 描述提示词发送前准备的临时媒体字节进度。
export interface MediaUploadProgressChangedDelta {
  // Client-generated submission identity used by the cancel action.
  // 用于取消动作的客户端提交标识。
  clientSubmissionId: string;
  // Existing conversation id, or null while a draft is being prepared.
  // 已有会话标识；草稿准备期间为空。
  conversationId?: string | null;
  // Exact non-secret route selected for this submission.
  // 本次提交选定的精确非秘密路由。
  providerId: string;
  serviceEntryId: string;
  connectionId: string;
  modelId: string;
  protocolProfileId: string;
  manifestRevision: number;
  manifestGeneration: number;
  routeRevision: string;
  operationId: string;
  uploadChannelId: string;
  // User-visible source metadata.
  // 面向用户展示的源文件元数据。
  fileName: string;
  mediaKind: string;
  fileIndex: number;
  fileCount: number;
  // Current transfer phase.
  // 当前传输阶段。
  phase: "preparing" | "uploading" | "completed" | "cancelled" | "failed";
  bytesSent: number;
  totalBytes?: number | null;
  error?: string | null;
}

// ConversationHydrationChangedDelta replaces conversation hydration progress without a full snapshot.
// ConversationHydrationChangedDelta 在不发送完整快照的情况下替换会话水合进度。
export interface ConversationHydrationChangedDelta {
  // Latest hydration progress payload.
  // 最新水合进度载荷。
  progress: ConversationHydrationProgress;
}

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
