import type {
  AgentSkillsCatalogSummary,
  ComposerAttachmentSummary,
  LuaSkillsCatalogSummary,
  LuaSkillsManagedRuntimeProgressSummary,
  LuaSkillsOperationProgressSummary,
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
import type { ContextCompactionWindowNodeId } from "./public-ai";
import type { AnswerCodeSnapshot, ConversationEventBody, ConversationEventSummary } from "@/service/conversation-event-window-store";

// ConversationAgentInteraction retains exact collaboration, authenticated question input or child-result facts for display only.
// ConversationAgentInteraction 仅为展示保留精确协作、已验证问题输入或子任务结果事实。
export type ConversationAgentInteraction = Omit<ConversationEventSummary, "event"> & {
  // User inputs enter only after host agent-message identity and recipient validation; human prose never selects a card.
  // 输入仅在验证宿主代理消息身份及接收方后进入；人类正文绝不选择卡片。
  event: Extract<ConversationEventBody, { kind: "collaboration" | "subagent_return" | "user_input" }>;
};

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
  // notificationClientId identifies the exact WebUI instance allowed to present this event.
  // notificationClientId 标识获准展示本事件通知的精确 WebUI 实例。
  notificationClientId?: string | null;
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
  // Whether this is a dedicated session; the kind is fixed at creation.
  // 是否为专职会话；该种类在创建时固定。
  dedicated?: boolean;
  // Floating-assistant name; a named dedicated session leaves the ordinary session list.
  // 浮动助手名称；具名的专职会话不再出现在普通会话列表中。
  assistantName?: string | null;
}

// ConversationMessage is the in-memory presentation record consumed by the restored conversation UI.
// ConversationMessage 是恢复后的会话界面消费的纯内存展示记录。
// It is projected only from V1 events and is never a persistence or RPC authority.
// 它仅由 V1 事件投影生成，绝不作为持久化或 RPC 权威。
export interface ConversationMessage {
  // Exact V1 interaction rendered separately from human prompts and assistant task completion.
  // 精确 V1 交互，与人类提示词及助手任务完成分开展示。
  agentInteraction?: ConversationAgentInteraction;
  // Stable presentation identity derived from the V1 event sequence.
  // 从 V1 事件序号派生的稳定展示身份。
  id: string;
  // Stable owning session identity.
  // 稳定的所属会话身份。
  conversationId: string;
  // Unix-millisecond timestamp text used by the restored visual timeline.
  // 恢复后的视觉时间线使用的 Unix 毫秒时间戳文本。
  createdAt?: string;
  // Presentation role derived from the typed V1 event kind.
  // 从强类型 V1 事件种类派生的展示角色。
  role: "user" | "assistant" | "system";
  // Optional display title retained for the branch presentation components.
  // 为分支展示组件保留的可选显示标题。
  title?: string | null;
  // Optional display record kind; it does not name a persisted legacy record.
  // 可选展示记录种类；它不代表任何旧版持久化记录。
  recordKind?: string | null;
  // Canonical visible body copied from the V1 event.
  // 从 V1 事件复制的规范可见正文。
  body: string;
  // Typed projection metadata used only by presentation components.
  // 仅供展示组件使用的强类型投影元数据。
  metadata?: unknown;
  // Answer-time `ui-code` captures of a sealed assistant text; absent while it streams.
  // 已封口 assistant 正文的回答当时 `ui-code` 抓取结果；流式期间不存在。
  answerCodeSnapshots?: readonly AnswerCodeSnapshot[];
  // Optional composer-compatible previews for process-local optimistic content.
  // 进程内乐观内容使用的可选输入区兼容预览。
  attachments?: ComposerAttachmentSummary[];
  // Whether the V1 generation boundary permits editing this root input.
  // V1 generation 边界是否允许编辑当前根输入。
  editableUserPrompt?: boolean;
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

// TransientReasoningSummaryItem is one provider-owned readable summary node.
// TransientReasoningSummaryItem 是一个由供应商持有的可读摘要节点。
export interface TransientReasoningSummaryItem {
  // Stable summary identity used for in-place updates.
  // 用于原地更新的稳定摘要标识。
  itemId: string;
  // Latest complete text for this exact summary node.
  // 当前精确摘要节点的最新完整文本。
  text: string;
}

// TransientReasoningStatus is one replace-in-place status excluded from transcript persistence.
// TransientReasoningStatus 是一条排除在转录持久化之外的原地替换式状态。
export interface TransientReasoningStatus {
  // Presentation kind selected by the response profile.
  // 由响应 Profile 选择的展示类型。
  kind: "summary" | "encrypted";
  // Ordered readable provider summary nodes when available.
  // 可用时的有序供应商可读摘要节点。
  summaries?: TransientReasoningSummaryItem[];
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

// SubagentPanelRouteResponse carries navigation after a subagent panel action; transcript data uses the V1 event window.
// SubagentPanelRouteResponse 携带子代理面板动作后的导航；转录数据使用 V1 事件窗口。
export interface SubagentPanelRouteResponse {
  // Parent conversation id that owns this route response.
  // 拥有本次路由响应的父会话标识。
  parentConversationId?: string | null;
  // Conversation id whose V1 event window should be displayed.
  // 应展示其 V1 事件窗口的会话标识。
  activeConversationId?: string | null;
  // Current subagent navigation, or null when no subagent panel is active.
  // 当前子代理导航；没有活动子代理面板时为空。
  subagentNavigation?: SubagentNavigationSnapshot | null;
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
  // Stable context-compaction window node selected by this session.
  // 当前会话选择的稳定上下文压缩窗口节点。
  sessionContextCompactionWindowNodeId?: ContextCompactionWindowNodeId | null;
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
  // Final enablement belongs to this exact session, not the service's shared selection.
  // 最终启用状态属于当前准确会话，而非服务端共享选择。
  memoryEnabled: boolean;
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

// SessionProviderUsageSummary carries live physical Provider accounting for one conversation.
// SessionProviderUsageSummary 携带单个会话的实时物理 Provider 统计。
export interface SessionProviderUsageSummary {
  /** Resident root lifetime counters, separated by owner.
   * 按所有者分离的常驻根生命周期计数。 */
  lifetime?: import("./session-lifetime-usage").SessionLifetimeUsage | null;
  /** Explicit availability error.
   * 明确的不可用原因。 */
  lifetimeError?: string | null;
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
  // Paired timing and output facts from measured session-owned physical requests.
  // 会话所属已测物理请求的配对计时与输出事实。
  outputTiming?: {
    // Number of measured requests.
    // 已测请求数量。
    requestCount: number;
    // Summed dispatch-to-first-token milliseconds.
    // 分发至首 Token 的累计毫秒数。
    firstTokenLatencyMs: number;
    // Summed first-token-to-completion milliseconds for the same requests.
    // 同一批请求首 Token 至完成的累计毫秒数。
    outputDurationMs: number;
    // Output tokens from exactly the measured requests.
    // 仅属于已测请求的输出 Token 数。
    outputTokens: number;
  } | null;
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
  | { kind: "sessionAddonsChanged"; payload: SessionAddonsChangedDelta }
  | { kind: "agentSkillsCatalogChanged"; payload: AgentSkillsCatalogChangedDelta }
  | { kind: "luaSkillsCatalogChanged"; payload: LuaSkillsCatalogChangedDelta }
  | { kind: "subagentCatalogChanged"; payload: SubagentCatalogChangedDelta }
  | { kind: "pluginCatalogChanged"; payload: PluginCatalogChangedDelta }
  | { kind: "projectSettingsChanged"; payload: ProjectSettingsChangedDelta }
  | {
    kind: "luaSkillsManagedRuntimeProgressChanged";
    payload: LuaSkillsManagedRuntimeProgressChangedDelta;
  }
  | {
    kind: "luaSkillsOperationProgressChanged";
    payload: LuaSkillsOperationProgressChangedDelta;
  }
  | { kind: "mediaUploadProgressChanged"; payload: MediaUploadProgressChangedDelta }
  | {
    kind: "toolRequestProgressChanged";
    payload: ToolRequestProgressChangedDelta;
  }
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
  domain: "vmm" | "agentSkills";
}

// LuaSkillsManagedRuntimeProgressChangedDelta replaces only the active managed-runtime progress.
// LuaSkillsManagedRuntimeProgressChangedDelta 仅替换当前受管运行时进度。
export interface LuaSkillsManagedRuntimeProgressChangedDelta {
  // Current native runtime installation progress, or null after the operation reaches a terminal state.
  // 当前原生运行时安装进度；操作到达终态后为 null。
  progress: LuaSkillsManagedRuntimeProgressSummary | null;
}

// LuaSkillsOperationProgressChangedDelta replaces only the active install or update operation progress.
// LuaSkillsOperationProgressChangedDelta 仅替换当前安装或更新操作进度。
export interface LuaSkillsOperationProgressChangedDelta {
  // Current operation progress, or null once no operation is bound.
  // 当前操作进度；不再有操作绑定时为 null。
  progress: LuaSkillsOperationProgressSummary | null;
}

// MediaUploadProgressChangedDelta describes ephemeral bytes transferred before dispatch or by a running media tool.
// MediaUploadProgressChangedDelta 描述分派前或运行中媒体工具传输的临时字节进度。
export interface MediaUploadProgressChangedDelta {
  // Unique transfer identity; cancellable pre-dispatch transfers reuse the client submission identity.
  // 唯一传输标识；可取消的分派前传输复用客户端提交标识。
  clientSubmissionId: string;
  // Existing conversation id, or null while a draft is being prepared.
  // 已有会话标识；草稿准备期间为空。
  conversationId?: string | null;
  // Stable managed attachment identity for composer-owned transfers.
  // 输入区持有传输使用的稳定托管附件身份。
  attachmentId?: string | null;
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
  phase: "preparing" | "uploading" | "awaiting_provider_confirmation" | "completed" | "cancelled" | "failed";
  bytesSent: number;
  totalBytes?: number | null;
  error?: string | null;
  // Whether this active transfer accepts the cancellation action.
  // 当前活动传输是否接受取消动作。
  cancellable?: boolean;
}

// ToolRequestProgressSnapshot describes one safe replace-in-place argument-formation snapshot.
// ToolRequestProgressSnapshot 描述一条安全的原地替换式参数构造快照。
export interface ToolRequestProgressSnapshot {
  // Exact durable API owning this transient draft.
  // 持有当前瞬态草稿的精确持久 API。
  apiCallId: number;
  // Stable host-side request stream identity.
  // 稳定的宿主侧请求流标识。
  requestStreamId: string;
  // Canonical tool-call identity known at this revision.
  // 当前修订已知的规范工具调用标识。
  toolCallId: string;
  // Exact public tool name frozen for this request.
  // 当前请求冻结的精确公开工具名称。
  toolName: string;
  // Closed request-formation state; execution states remain in durable transcript records.
  // 封闭的请求构造状态；执行状态仍由持久转录记录持有。
  state: "receiving" | "arguments_assembled" | "ready";
  // Exact UTF-8 byte and Unicode character counts received so far.
  // 当前已接收的精确 UTF-8 字节数与 Unicode 字符数。
  receivedBytes: number;
  receivedCharacters: number;
  // Monotonic request-local revision.
  // 请求局部的单调修订号。
  requestRevision: number;
  // Optional sanitized preview update; absent/null leaves the current request preview unchanged.
  // 可选的净化预览更新；缺席或 null 保留当前请求预览。
  // The whole progress being null, durable handoff or snapshot reset clears retained previews.
  // 整条 progress 为 null、持久化交接或快照重置会清理保留的预览。
  preview?: Record<string, unknown> | null;
}

// ToolRequestProgressChangedDelta replaces or clears one leaf-only tool request snapshot.
// ToolRequestProgressChangedDelta 替换或清除单个仅叶子级工具请求快照。
export interface ToolRequestProgressChangedDelta {
  // Conversation identity owning the request.
  // 持有当前请求的会话标识。
  conversationId: string;
  // Stable request-stream identity used for replacement and clearing.
  // 用于替换与清除的稳定请求流标识。
  requestStreamId: string;
  // Latest safe snapshot, or null when validation fails or the request is aborted.
  // 最新安全快照；校验失败或请求中止后为空。
  progress: ToolRequestProgressSnapshot | null;
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
