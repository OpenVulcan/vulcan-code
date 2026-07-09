import {
  applyAppendMessageBodyDelta,
  applyConversationMessagesPatchedDelta,
  groupMessagesByConversation,
  mergeIncomingConversationMessages,
  updateWorkbenchSubagentNavigationRunPhase,
} from "@/state/workbench-message-merge";
import type {
  ConversationHydrationProgress,
  ContextInjectionChangedDelta,
  ConversationMessage,
  ConversationSummary,
  PendingPromptsChangedDelta,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  SessionAddonsChangedResponse,
  SessionAddonsChangedDelta,
  SessionVmmPolicyByConversation,
  SubagentNavigationSnapshot,
  SubagentPanelRouteResponse,
  UpdateConversationRunPhaseDelta,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

export {
  coalesceWorkbenchDeltaEnvelopes,
  workbenchDeltaConversationId,
} from "@/state/workbench-message-merge";

// InflightSubagentOpenRequest tracks one backend transcript-load command that duplicate clicks can share.
// InflightSubagentOpenRequest 跟踪一条可被重复点击复用的后端转录加载命令。
export interface InflightSubagentOpenRequest {
  // Stable parent/task route key for request coalescing.
  // 用于请求合并的稳定父级/任务键。
  key: string;
  // Monotonic route request sequence owned by the original backend call.
  // 原始后端调用持有的单调路由请求序号。
  requestSeq: number;
  // Backend command promise for the transcript load.
  // 转录加载对应的后端命令 Promise。
  promise: Promise<SubagentPanelRouteResponse | null>;
}

// SubagentSurfaceMessageCache stores loaded child transcript messages by child task conversation id.
// SubagentSurfaceMessageCache 按子任务会话标识保存已载入的子代理转录消息。
export type SubagentSurfaceMessageCache = Map<string, ConversationMessage[]>;

// WorkbenchMergeOptions tunes snapshot merge behavior for local interaction state.
// WorkbenchMergeOptions 调整快照合并时本地交互状态的处理方式。
export interface WorkbenchMergeOptions {
  // Whether local project expansion should win over incoming snapshots.
  // 本地项目展开状态是否优先于传入快照。
  preserveExpandedProjectIds?: boolean;
  // Controls whether selectedConversationId comes from local UI state or the incoming service snapshot.
  // 控制 selectedConversationId 来自本地界面状态还是传入的服务端快照。
  selectedConversationSource?: "local" | "incoming";
  // Focused conversation id for session-scoped hydration responses.
  // 会话范围水合响应对应的明确目标会话标识。
  focusedConversationId?: string | null;
}

// shouldAcceptWorkbenchRevision returns whether one incoming service revision can mutate UI state.
// shouldAcceptWorkbenchRevision 判断传入服务版本是否允许修改界面状态。
export function shouldAcceptWorkbenchRevision(currentRevision: number, incomingRevision: number): boolean {
  return currentRevision <= 0 || incomingRevision > currentRevision;
}

// visibleConversationId returns a candidate only when it still exists in the provided sidebar list.
// visibleConversationId 仅在候选会话仍存在于给定侧栏列表时返回该候选。
//
// Parameters:
// 参数：
// conversations is the authoritative conversation list for this projection.
// conversations 是本次投影使用的权威会话列表。
// candidate is the selected conversation candidate to validate.
// candidate 是需要验证的选中会话候选值。
//
// Returns:
// 返回值：
// The candidate id when it is visible in conversations; otherwise null.
// 当候选标识存在于会话列表中时返回该标识，否则返回 null。
function visibleConversationId(
  conversations: ConversationSummary[],
  candidate: string | null | undefined,
): string | null {
  return candidate && conversations.some((conversation) => conversation.id === candidate)
    ? candidate
    : null;
}

// mergeWorkbenchSnapshot merges backend workbench updates without discarding lazily loaded messages.
// mergeWorkbenchSnapshot 合并后端工作台更新且不丢弃已懒加载的会话正文。
export function mergeWorkbenchSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  options: WorkbenchMergeOptions = {},
): WorkbenchSnapshot {
  const currentRevision = current.revision ?? 0;
  const incomingRevision = incoming.revision ?? 0;
  if (!shouldAcceptWorkbenchRevision(currentRevision, incomingRevision)) {
    return current;
  }

  const incomingConversationIds = new Set(incoming.messages.map((message) => message.conversationId));
  const currentMessagesByConversation = groupMessagesByConversation(current.messages);
  const incomingMessagesByConversation = groupMessagesByConversation(incoming.messages);
  const incomingConversationsById = new Map(incoming.conversations.map((conversation) => [conversation.id, conversation]));
  const currentConversationsById = new Map(current.conversations.map((conversation) => [conversation.id, conversation]));
  const incomingProjectIds = new Set(incoming.projects.map((project) => project.id));
  const expandedProjectSource = options.preserveExpandedProjectIds === false
    ? incoming.expandedProjectIds ?? current.expandedProjectIds
    : current.expandedProjectIds ?? incoming.expandedProjectIds;
  const expandedProjectIds = (expandedProjectSource ?? [])
    .filter((projectId) => incomingProjectIds.has(projectId));
  const mergedIncomingMessages = [...incomingMessagesByConversation.entries()].flatMap(([conversationId, messages]) => (
    mergeIncomingConversationMessages(
      currentMessagesByConversation.get(conversationId) ?? [],
      messages,
      currentConversationsById.get(conversationId),
      incomingConversationsById.get(conversationId),
    )
  ));
  const incomingMessageIds = new Set(mergedIncomingMessages.map((message) => message.id));
  const retainedMessages = current.messages.filter((message) => (
    !incomingConversationIds.has(message.conversationId) && !incomingMessageIds.has(message.id)
  ));
  const incomingSelectedConversationId = visibleConversationId(
    incoming.conversations,
    incoming.selectedConversationId,
  );
  const localSelectedConversationId = visibleConversationId(
    incoming.conversations,
    current.selectedConversationId,
  );
  const selectedConversationId = options.selectedConversationSource === "incoming"
    ? incomingSelectedConversationId ?? localSelectedConversationId ?? incoming.conversations[0]?.id ?? null
    : localSelectedConversationId ?? incomingSelectedConversationId ?? incoming.conversations[0]?.id ?? null;
  const conversationHydration = selectMergedConversationHydration(
    current.conversationHydration ?? null,
    incoming.conversationHydration ?? null,
    selectedConversationId,
  );
  const visibleConversationIds = new Set(incoming.conversations.map((conversation) => conversation.id));
  const incomingSubagentNavigations = deriveIncomingSubagentNavigations(incoming);
  const mergedWorkbench = {
    ...incoming,
    selectedConversationId,
    expandedProjectIds,
    conversationHydration,
    hydratedConversationWindows: mergeHydratedConversationWindows(
      current.hydratedConversationWindows ?? [],
      incoming.hydratedConversationWindows ?? [],
      visibleConversationIds,
    ),
    conversations: mergeConversationSummaries(current.conversations, incoming.conversations, selectedConversationId),
    messages: [...retainedMessages, ...mergedIncomingMessages],
    pendingPromptsByConversation: incoming.pendingPromptsByConversation ?? current.pendingPromptsByConversation ?? [],
    pendingQuestions: incoming.pendingQuestions ?? current.pendingQuestions ?? [],
    sessionAddonsByConversation: incoming.sessionAddonsByConversation ?? current.sessionAddonsByConversation ?? [],
    sessionVmmPoliciesByConversation: incoming.sessionVmmPoliciesByConversation ?? current.sessionVmmPoliciesByConversation ?? [],
    contextInjectionByConversation: mergeContextInjectionByConversation(
      current.contextInjectionByConversation ?? [],
      incoming.contextInjectionByConversation ?? [],
    ),
    subagentNavigationByConversation: mergeSubagentNavigationsByConversation(
      current.subagentNavigationByConversation ?? [],
      incomingSubagentNavigations,
      focusedSubagentNavigationClearIds(incomingSubagentNavigations, options.focusedConversationId ?? null),
    ),
  };
  return deriveSelectedConversationFields({
    ...mergedWorkbench,
    conversations: applyNormalizedConversationAttention(mergedWorkbench.conversations, mergedWorkbench),
  });
}

// mergeConversationHydrationSnapshot merges a focused conversation hydration response.
// mergeConversationHydrationSnapshot 合并一次聚焦会话正文水合响应。
//
// Parameters:
// 参数：
// current is the UI snapshot currently rendered by the workbench.
// current 是工作台当前正在渲染的界面快照。
// incoming is the service snapshot returned by a conversation hydration RPC.
// incoming 是会话正文水合 RPC 返回的服务快照。
// options carries the same local-state preferences used by full snapshot merging.
// options 携带与完整快照合并相同的本地状态偏好。
//
// Returns:
// 返回值：
// A full merge when the service revision is fresh; otherwise a session-scoped merge.
// 当服务版本较新时返回完整合并结果，否则返回会话范围内的合并结果。
export function mergeConversationHydrationSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  options: WorkbenchMergeOptions = {},
): WorkbenchSnapshot {
  const currentRevision = current.revision ?? 0;
  const incomingRevision = incoming.revision ?? 0;
  if (shouldAcceptWorkbenchRevision(currentRevision, incomingRevision)) {
    return mergeWorkbenchSnapshot(current, incoming, options);
  }
  return mergeStaleConversationHydrationSnapshot(current, incoming, options.focusedConversationId ?? null);
}

// mergeStaleConversationHydrationSnapshot merges only conversation-local payload from an older hydration response.
// mergeStaleConversationHydrationSnapshot 只合并旧水合响应中的会话局部载荷。
//
// Parameters:
// 参数：
// current is the authoritative UI snapshot whose global fields must be preserved.
// current 是必须保留全局字段的权威界面快照。
// incoming is the older hydration snapshot carrying focused conversation data.
// incoming 是携带聚焦会话数据的较旧水合快照。
// focusedConversationId is the explicit session id from the RPC response.
// focusedConversationId 是 RPC 响应携带的明确会话标识。
//
// Returns:
// 返回值：
// A snapshot with focused messages, hydration windows, progress, and subagent navigation merged in.
// 返回已合入聚焦消息、水合窗口、进度与子代理导航的快照。
function mergeStaleConversationHydrationSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  focusedConversationId: string | null,
): WorkbenchSnapshot {
  const hydratedConversationIds = conversationHydrationPayloadIds(incoming, focusedConversationId);
  if (hydratedConversationIds.size === 0) {
    return current;
  }

  const currentMessagesByConversation = groupMessagesByConversation(current.messages);
  const incomingMessagesByConversation = groupMessagesByConversation(incoming.messages);
  const incomingMessageConversationIds = new Set(
    [...incomingMessagesByConversation.keys()].filter((conversationId) => hydratedConversationIds.has(conversationId)),
  );
  const currentConversationsById = new Map(current.conversations.map((conversation) => [conversation.id, conversation]));
  const incomingConversationsById = new Map(incoming.conversations.map((conversation) => [conversation.id, conversation]));
  const mergedIncomingMessages = [...incomingMessagesByConversation.entries()]
    .filter(([conversationId]) => incomingMessageConversationIds.has(conversationId))
    .flatMap(([conversationId, messages]) => mergeIncomingConversationMessages(
      currentMessagesByConversation.get(conversationId) ?? [],
      messages,
      currentConversationsById.get(conversationId),
      incomingConversationsById.get(conversationId),
  ));
  const incomingMessageIds = new Set(mergedIncomingMessages.map((message) => message.id));
  const retainedMessages = current.messages.filter((message) => (
    !incomingMessageConversationIds.has(message.conversationId) && !incomingMessageIds.has(message.id)
  ));
  const visibleConversationIds = new Set(current.conversations.map((conversation) => conversation.id));
  const incomingWindows = (incoming.hydratedConversationWindows ?? [])
    .filter((window) => hydratedConversationIds.has(window.sessionId));
  const incomingNavigations = deriveIncomingSubagentNavigations(incoming)
    .filter((navigation) => hydratedConversationIds.has(navigation.parentConversationId));
  const incomingHydration = incoming.conversationHydration
    && hydratedConversationIds.has(incoming.conversationHydration.sessionId)
    ? incoming.conversationHydration
    : null;
  const mergedWorkbench = {
    ...current,
    conversationHydration: selectMergedConversationHydration(
      current.conversationHydration ?? null,
      incomingHydration,
      current.selectedConversationId ?? null,
    ),
    hydratedConversationWindows: mergeHydratedConversationWindows(
      current.hydratedConversationWindows ?? [],
      incomingWindows,
      visibleConversationIds,
    ),
    subagentNavigationByConversation: mergeSubagentNavigationsByConversation(
      current.subagentNavigationByConversation ?? [],
      incomingNavigations,
      focusedSubagentNavigationClearIds(incomingNavigations, focusedConversationId),
    ),
    messages: [...retainedMessages, ...mergedIncomingMessages],
  };
  return deriveSelectedConversationFields({
    ...mergedWorkbench,
    conversations: applyNormalizedConversationAttention(mergedWorkbench.conversations, mergedWorkbench),
  });
}

// conversationHydrationPayloadIds finds the conversation ids that a hydration response may safely update.
// conversationHydrationPayloadIds 找出一次水合响应允许安全更新的会话标识。
//
// Parameters:
// 参数：
// incoming is the hydration snapshot to inspect.
// incoming 是需要检查的水合快照。
// focusedConversationId is the explicit target id supplied by the caller.
// focusedConversationId 是调用方提供的明确目标标识。
//
// Returns:
// 返回值：
// A set containing message, progress, and selected conversation ids from the payload.
// 返回包含消息、进度与选中会话标识的集合。
function conversationHydrationPayloadIds(
  incoming: WorkbenchSnapshot,
  focusedConversationId: string | null,
): Set<string> {
  const ids = new Set<string>();
  if (focusedConversationId) {
    ids.add(focusedConversationId);
    return ids;
  }
  for (const message of incoming.messages) {
    ids.add(message.conversationId);
  }
  if (incoming.selectedConversationId) {
    ids.add(incoming.selectedConversationId);
  }
  if (incoming.conversationHydration?.sessionId) {
    ids.add(incoming.conversationHydration.sessionId);
  }
  return ids;
}

// selectMergedConversationHydration keeps background conversation loads from overwriting the selected load progress.
// selectMergedConversationHydration 防止后台会话加载进度覆盖当前选中会话的加载进度。
function selectMergedConversationHydration(
  current: ConversationHydrationProgress | null,
  incoming: ConversationHydrationProgress | null,
  selectedConversationId: string | null,
): ConversationHydrationProgress | null {
  if (!incoming) {
    return current?.sessionId === selectedConversationId ? current : null;
  }
  if (!selectedConversationId || incoming.sessionId === selectedConversationId) {
    return incoming;
  }
  if (current?.sessionId === selectedConversationId) {
    return current;
  }
  return incoming;
}

// mergeHydratedConversationWindows preserves retained conversation body windows across partial snapshots.
// mergeHydratedConversationWindows 在局部快照之间保留已保留的会话正文窗口。
//
// Parameters:
// 参数：
// currentWindows are the UI-known retained conversation windows before the backend update.
// currentWindows 是后端更新前 UI 已知的保留会话窗口。
// incomingWindows are retained conversation windows carried by the incoming backend snapshot.
// incomingWindows 是当前后端快照携带的保留会话窗口。
// visibleConversationIds are conversation ids still present in the incoming sidebar summary.
// visibleConversationIds 是当前侧栏摘要中仍然存在的会话标识。
//
// Returns:
// 返回值：
// A per-session merged window list that keeps known hydrated state for still-visible conversations.
// 返回按会话合并后的窗口列表，为仍可见会话保留已知水合状态。
function mergeHydratedConversationWindows(
  currentWindows: NonNullable<WorkbenchSnapshot["hydratedConversationWindows"]>,
  incomingWindows: NonNullable<WorkbenchSnapshot["hydratedConversationWindows"]>,
  visibleConversationIds: Set<string>,
): NonNullable<WorkbenchSnapshot["hydratedConversationWindows"]> {
  const merged = new Map(currentWindows.map((window) => [window.sessionId, window]));
  for (const window of incomingWindows) {
    merged.set(window.sessionId, window);
  }
  return [...merged.values()].filter((window) => visibleConversationIds.has(window.sessionId));
}

// deriveSelectedConversationFields projects top-level selected-session fields from normalized per-session caches.
// deriveSelectedConversationFields 从按会话归一化缓存派生顶层当前会话字段。
function deriveSelectedConversationFields(workbench: WorkbenchSnapshot): WorkbenchSnapshot {
  const selectedConversationId = workbench.selectedConversationId ?? null;
  const pendingQuestion = selectedConversationId
    ? (workbench.pendingQuestions ?? []).find((question) => question.sessionId === selectedConversationId) ?? null
    : null;
  const pendingPrompts = selectedConversationId
    ? workbench.pendingPromptsByConversation?.find((entry) => entry.conversationId === selectedConversationId)?.pendingPrompts ?? []
    : [];
  const sessionAddons = selectedConversationId
    ? workbench.sessionAddonsByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  const sessionVmmPolicyEntry = selectedConversationId
    ? workbench.sessionVmmPoliciesByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  const subagentNavigation = selectedConversationId
    ? workbench.subagentNavigationByConversation?.find((navigation) => navigation.parentConversationId === selectedConversationId) ?? null
    : null;
  const contextInjectionEntry = selectedConversationId
    ? workbench.contextInjectionByConversation?.find((entry) => entry.conversationId === selectedConversationId) ?? null
    : null;
  return {
    ...workbench,
    pendingQuestion,
    pendingPrompts,
    sessionGoal: sessionAddons?.sessionGoal ?? null,
    sessionTodolist: sessionAddons?.sessionTodolist ?? null,
    sessionPlanMode: sessionAddons?.sessionPlanMode ?? null,
    sessionVmmPolicy: sessionVmmPolicyEntry?.sessionVmmPolicy ?? null,
    subagentNavigation,
    contextInjection: contextInjectionEntry?.contextInjection ?? null,
  };
}

// workbenchWithSelectedConversationProjection reprojects selected-session fields for a UI-local conversation choice.
// workbenchWithSelectedConversationProjection 根据 UI 本地会话选择重新投影当前会话字段。
//
// Parameters:
// 参数：
// workbench is the normalized snapshot to project.
// workbench 是需要投影的规范化快照。
// selectedConversationId is the UI-local selected conversation id.
// selectedConversationId 是 UI 本地选中的会话标识。
//
// Returns:
// 返回值：
// A workbench whose top-level selected-session fields match the visible selected conversation, or are cleared.
// 返回顶层当前会话字段匹配可见选中会话或已清空的工作台快照。
export function workbenchWithSelectedConversationProjection(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
): WorkbenchSnapshot {
  const visibleSelectedConversationId = visibleConversationId(
    workbench.conversations,
    selectedConversationId,
  );
  return deriveSelectedConversationFields({
    ...workbench,
    selectedConversationId: visibleSelectedConversationId,
  });
}

// upsertPendingPromptsForConversation upserts or removes one conversation prompt queue.
// upsertPendingPromptsForConversation 写入或删除单个会话的待发送提示词队列。
function upsertPendingPromptsForConversation(
  current: PendingPromptsChangedDelta[],
  entry: PendingPromptsChangedDelta,
): PendingPromptsChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.pendingPrompts.length > 0 ? [...next, entry] : next;
}

// upsertSessionAddonsForConversation upserts or removes one session addon projection.
// upsertSessionAddonsForConversation 写入或删除单个会话附属投影。
function upsertSessionAddonsForConversation(
  current: SessionAddonsChangedDelta[],
  entry: SessionAddonsChangedDelta,
): SessionAddonsChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  const hasAddon = Boolean(entry.sessionGoal || entry.sessionTodolist || entry.sessionPlanMode);
  return hasAddon ? [...next, entry] : next;
}

// upsertSessionVmmPolicyForConversation upserts or removes one conversation VMM policy projection.
// upsertSessionVmmPolicyForConversation 写入或删除单个会话的 VMM 策略投影。
function upsertSessionVmmPolicyForConversation(
  current: SessionVmmPolicyByConversation[],
  entry: SessionAddonsChangedDelta,
): SessionVmmPolicyByConversation[] {
  if (entry.sessionVmmPolicy === undefined) {
    return current;
  }
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.sessionVmmPolicy ? [...next, {
    conversationId: entry.conversationId,
    sessionVmmPolicy: entry.sessionVmmPolicy,
  }] : next;
}

// mergeContextInjectionByConversation merges memory-panel projections by conversation id.
// mergeContextInjectionByConversation 按会话标识合并记忆面板投影。
function mergeContextInjectionByConversation(
  current: ContextInjectionChangedDelta[],
  incoming: ContextInjectionChangedDelta[],
): ContextInjectionChangedDelta[] {
  const merged = new Map(current.map((entry) => [entry.conversationId, entry]));
  for (const entry of incoming) {
    merged.set(entry.conversationId, entry);
  }
  return [...merged.values()];
}

// upsertContextInjectionForConversation upserts or removes one memory-panel projection.
// upsertContextInjectionForConversation 写入或删除单个会话的记忆面板投影。
function upsertContextInjectionForConversation(
  current: ContextInjectionChangedDelta[],
  entry: ContextInjectionChangedDelta,
): ContextInjectionChangedDelta[] {
  const next = current.filter((candidate) => candidate.conversationId !== entry.conversationId);
  return entry.contextInjection ? [...next, entry] : next;
}

// deriveIncomingSubagentNavigations extracts full-snapshot subagent navigation cache entries.
// deriveIncomingSubagentNavigations 提取全量快照中的子代理导航缓存条目。
function deriveIncomingSubagentNavigations(workbench: WorkbenchSnapshot): SubagentNavigationSnapshot[] {
  const entries = workbench.subagentNavigationByConversation ?? [];
  return workbench.subagentNavigation ? [...entries, workbench.subagentNavigation] : entries;
}

// mergeSubagentNavigationsByConversation merges subagent navigation snapshots by parent conversation id and applies explicit clears first.
// mergeSubagentNavigationsByConversation 按父会话标识合并子代理导航快照，并先应用显式清理。
function mergeSubagentNavigationsByConversation(
  current: SubagentNavigationSnapshot[],
  incoming: SubagentNavigationSnapshot[],
  clearParentConversationIds: Set<string> = new Set(),
): SubagentNavigationSnapshot[] {
  const merged = new Map(current.map((navigation) => [navigation.parentConversationId, navigation]));
  for (const parentConversationId of clearParentConversationIds) {
    merged.delete(parentConversationId);
  }
  for (const navigation of incoming) {
    merged.set(navigation.parentConversationId, navigation);
  }
  return [...merged.values()];
}

// focusedSubagentNavigationClearIds returns the focused parent ids whose missing navigation is authoritative.
// focusedSubagentNavigationClearIds 返回缺失导航具有权威删除语义的聚焦父会话标识。
function focusedSubagentNavigationClearIds(
  incoming: SubagentNavigationSnapshot[],
  focusedConversationId: string | null,
): Set<string> {
  if (!focusedConversationId) {
    return new Set();
  }
  return incoming.some((navigation) => navigation.parentConversationId === focusedConversationId)
    ? new Set()
    : new Set([focusedConversationId]);
}

// upsertSubagentNavigationForConversation upserts or removes one parent conversation navigation snapshot.
// upsertSubagentNavigationForConversation 写入或删除单个父会话的子代理导航快照。
function upsertSubagentNavigationForConversation(
  current: SubagentNavigationSnapshot[],
  parentConversationId: string | null,
  navigation: SubagentNavigationSnapshot | null,
): SubagentNavigationSnapshot[] {
  if (!parentConversationId) {
    return current;
  }
  const next = current.filter((candidate) => candidate.parentConversationId !== parentConversationId);
  return navigation ? [...next, navigation] : next;
}

// applyPendingQuestionAttention updates sidebar ask markers from the normalized ask queue.
// applyPendingQuestionAttention 根据归一化 ask 队列更新侧栏提问标记。
function applyPendingQuestionAttention(
  conversations: ConversationSummary[],
  pendingQuestions: PendingQuestionSnapshot[],
): ConversationSummary[] {
  const pendingBySession = new Map(pendingQuestions.map((question) => [question.sessionId, question]));
  return conversations.map((conversation) => {
    const pendingQuestion = pendingBySession.get(conversation.id);
    if (pendingQuestion) {
      return {
        ...conversation,
        attention: {
          kind: "ask",
          priority: 90,
          requestId: pendingQuestion.id,
          label: "Ask question waiting",
        },
      };
    }
    if (conversation.attention?.kind !== "ask") {
      return conversation;
    }
    return {
      ...conversation,
      attention: conversation.unread
        ? {
          kind: "unread",
          priority: 10,
          label: "Unread activity",
        }
        : null,
    };
  });
}

// pendingQuestionsFromDelta returns the next normalized ask queue from a full or single-session delta.
// pendingQuestionsFromDelta 根据全量或单会话增量返回下一版归一化提问队列。
function pendingQuestionsFromDelta(
  current: PendingQuestionSnapshot[],
  conversationId: string | null | undefined,
  pendingQuestion: PendingQuestionSnapshot | null | undefined,
  pendingQuestions: PendingQuestionSnapshot[] | undefined,
): PendingQuestionSnapshot[] {
  if (pendingQuestions) {
    return pendingQuestions;
  }
  if (!conversationId) {
    return pendingQuestion ? [pendingQuestion] : [];
  }
  const nextQuestions = current.filter((question) => question.sessionId !== conversationId);
  if (pendingQuestion) {
    nextQuestions.push(pendingQuestion);
  }
  return nextQuestions.sort((left, right) => left.sequence - right.sequence);
}

// applySessionAddonAttention updates one sidebar Plan marker from session addon state.
// applySessionAddonAttention 根据会话附属状态更新单个侧栏计划标记。
function applySessionAddonAttention(
  conversations: ConversationSummary[],
  entry: SessionAddonsChangedDelta,
): ConversationSummary[] {
  return conversations.map((conversation) => {
    if (conversation.id !== entry.conversationId || conversation.attention?.kind === "ask") {
      return conversation;
    }
    if (entry.sessionPlanMode?.executionMode?.toLowerCase() === "plan") {
      return {
        ...conversation,
        attention: {
          kind: "plan_confirm",
          priority: 80,
          requestId: entry.sessionPlanMode.artifact?.path ?? null,
          label: "Plan mode waiting",
        },
      };
    }
    if (conversation.attention?.kind !== "plan_confirm") {
      return conversation;
    }
    return {
      ...conversation,
      attention: conversation.unread
        ? {
          kind: "unread",
          priority: 10,
          label: "Unread activity",
        }
        : null,
    };
  });
}

// applyNormalizedConversationAttention reapplies per-session ask and Plan markers after summary replacement.
// applyNormalizedConversationAttention 在摘要替换后重新应用按会话归一化的提问与计划标记。
function applyNormalizedConversationAttention(
  conversations: ConversationSummary[],
  workbench: WorkbenchSnapshot,
): ConversationSummary[] {
  let nextConversations = conversations;
  for (const entry of workbench.sessionAddonsByConversation ?? []) {
    nextConversations = applySessionAddonAttention(nextConversations, entry);
  }
  return applyPendingQuestionAttention(nextConversations, workbench.pendingQuestions ?? []);
}

// applyWorkbenchDeltaWithSubagentSurfaceMessageCache applies one partial update while keeping child transcript caches coherent.
// applyWorkbenchDeltaWithSubagentSurfaceMessageCache 应用单条局部更新，并保持子代理转录缓存一致。
export function applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  envelope: WorkbenchDeltaEnvelope,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  if (envelope.generation <= (workbench.revision ?? 0)) {
    return workbench;
  }
  const nextWorkbench = applyWorkbenchDeltaPayloadWithSubagentSurfaceMessageCache(workbench, envelope, cache);
  return {
    ...nextWorkbench,
    revision: envelope.generation,
  };
}

// applyWorkbenchDeltaPayloadWithSubagentSurfaceMessageCache applies one delta payload before revision stamping.
// applyWorkbenchDeltaPayloadWithSubagentSurfaceMessageCache 在写入版本号前应用单条增量载荷。
function applyWorkbenchDeltaPayloadWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  envelope: WorkbenchDeltaEnvelope,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  switch (envelope.delta.kind) {
    case "appendMessageBody": {
      const nextWorkbench = applyAppendMessageBodyDelta(workbench, envelope.delta.payload);
      syncSubagentSurfaceMessageCacheForDelta(cache, nextWorkbench, envelope.delta.payload.conversationId);
      return injectCachedSubagentSurfaceMessages(nextWorkbench, cache);
    }
    case "updateConversationRunPhase":
      return applyUpdateConversationRunPhaseDelta(workbench, envelope.delta.payload);
    case "pendingPromptsChanged":
      return deriveSelectedConversationFields({
        ...workbench,
        pendingPromptsByConversation: upsertPendingPromptsForConversation(
          workbench.pendingPromptsByConversation ?? [],
          envelope.delta.payload,
        ),
      });
    case "conversationSummaryChanged":
      return applyConversationSummaryChangedDelta(workbench, envelope.delta.payload);
    case "pendingQuestionChanged": {
      const pendingQuestions = pendingQuestionsFromDelta(
        workbench.pendingQuestions ?? [],
        envelope.delta.payload.conversationId ?? null,
        envelope.delta.payload.pendingQuestion ?? null,
        envelope.delta.payload.pendingQuestions,
      );
      return deriveSelectedConversationFields({
        ...workbench,
        pendingQuestions,
        conversations: applyPendingQuestionAttention(workbench.conversations, pendingQuestions),
      });
    }
    case "commandSessionsChanged":
      return {
        ...workbench,
        commandSessions: envelope.delta.payload.commandSessions,
      };
    case "subagentNavigationChanged":
      return injectCachedSubagentSurfaceMessages(deriveSelectedConversationFields({
        ...workbench,
        subagentNavigationByConversation: upsertSubagentNavigationForConversation(
          workbench.subagentNavigationByConversation ?? [],
          envelope.delta.payload.parentConversationId
            ?? envelope.delta.payload.subagentNavigation?.parentConversationId
            ?? null,
          envelope.delta.payload.subagentNavigation ?? null,
        ),
      }), cache);
    case "contextUsageChanged":
      return {
        ...workbench,
        contextUsage: envelope.delta.payload.contextUsage ?? null,
      };
    case "contextInjectionChanged":
      return applyContextInjectionChangedDelta(workbench, envelope.delta.payload);
    case "conversationMessagesChanged":
      return mergeConversationMessagesWithSubagentSurfaceMessageCache(
        workbench,
        envelope.delta.payload.conversationId,
        envelope.delta.payload.messages,
        cache,
      );
    case "conversationMessagesPatched": {
      const nextWorkbench = applyConversationMessagesPatchedDelta(workbench, envelope.delta.payload);
      syncSubagentSurfaceMessageCacheForDelta(cache, nextWorkbench, envelope.delta.payload.conversationId);
      return injectCachedSubagentSurfaceMessages(nextWorkbench, cache);
    }
    case "sessionAddonsChanged":
      return deriveSelectedConversationFields({
        ...workbench,
        conversations: applySessionAddonAttention(workbench.conversations, envelope.delta.payload),
        sessionAddonsByConversation: upsertSessionAddonsForConversation(
          workbench.sessionAddonsByConversation ?? [],
          envelope.delta.payload,
        ),
        sessionVmmPoliciesByConversation: upsertSessionVmmPolicyForConversation(
          workbench.sessionVmmPoliciesByConversation ?? [],
          envelope.delta.payload,
        ),
      });
    case "conversationHydrationChanged":
      return {
        ...workbench,
        conversationHydration: envelope.delta.payload.progress,
      };
  }
}

// applyConversationSummaryChangedDelta upserts one sidebar summary without touching transcript rows.
// applyConversationSummaryChangedDelta 写入单个侧栏摘要，不触碰转录消息行。
function applyConversationSummaryChangedDelta(
  workbench: WorkbenchSnapshot,
  delta: { conversation: ConversationSummary; selectedConversationId?: string | null },
): WorkbenchSnapshot {
  let found = false;
  const conversations = workbench.conversations.map((conversation) => {
    if (conversation.id !== delta.conversation.id) {
      return conversation;
    }
    found = true;
    return delta.conversation;
  });
  if (!found) {
    conversations.unshift(delta.conversation);
  }
  const nextWorkbench = {
    ...workbench,
    conversations,
  };
  return {
    ...nextWorkbench,
    conversations: applyNormalizedConversationAttention(conversations, nextWorkbench),
  };
}

// applyUpdateConversationRunPhaseDelta updates one conversation lifecycle row without touching transcript messages.
// applyUpdateConversationRunPhaseDelta 在不触碰转录消息的情况下更新单个会话生命周期行。
function applyUpdateConversationRunPhaseDelta(
  workbench: WorkbenchSnapshot,
  delta: UpdateConversationRunPhaseDelta,
): WorkbenchSnapshot {
  const subagentNavigationProjection = updateWorkbenchSubagentNavigationRunPhase(workbench, delta.conversationId, delta.runPhase);
  return deriveSelectedConversationFields({
    ...workbench,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === delta.conversationId
        ? {
            ...conversation,
            runPhase: delta.runPhase,
            status: delta.status ?? conversation.status,
            lastUpdatedLabel: delta.lastUpdatedLabel ?? conversation.lastUpdatedLabel,
          }
        : conversation
    )),
    ...subagentNavigationProjection,
  });
}

// applyContextInjectionChangedDelta replaces the selected conversation memory-panel projection.
// applyContextInjectionChangedDelta 替换当前选中会话的记忆面板投影。
function applyContextInjectionChangedDelta(
  workbench: WorkbenchSnapshot,
  delta: ContextInjectionChangedDelta,
): WorkbenchSnapshot {
  return deriveSelectedConversationFields({
    ...workbench,
    contextInjectionByConversation: upsertContextInjectionForConversation(
      workbench.contextInjectionByConversation ?? [],
      delta,
    ),
  });
}

// syncSubagentSurfaceMessageCacheForDelta updates the cached child transcript touched by one delta.
// syncSubagentSurfaceMessageCacheForDelta 更新单条增量影响的子代理转录缓存。
function syncSubagentSurfaceMessageCacheForDelta(
  cache: SubagentSurfaceMessageCache,
  workbench: WorkbenchSnapshot,
  conversationId: string,
): void {
  const belongsToVisibleTask = workbench.subagentNavigation?.tasks?.some((task) => task.taskId === conversationId) ?? false;
  if (!belongsToVisibleTask && !cache.has(conversationId)) {
    return;
  }
  cache.set(
    conversationId,
    workbench.messages.filter((message) => message.conversationId === conversationId),
  );
}

// workbenchWithSubagentSurfaceMessageCache absorbs and injects cached child transcript windows for one snapshot.
// workbenchWithSubagentSurfaceMessageCache 为单个快照吸收并注入已缓存的子代理转录窗口。
export function workbenchWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  rememberSubagentSurfaceMessages(cache, workbench);
  return injectCachedSubagentSurfaceMessages(workbench, cache);
}

// mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache merges one backend snapshot while preserving all loaded child surfaces.
// mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache 合并后端快照，同时保留所有已载入的子代理界面。
export function mergeWorkbenchSnapshotWithSubagentSurfaceMessageCache(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
  options?: WorkbenchMergeOptions,
): WorkbenchSnapshot {
  const mergedWorkbench = mergeWorkbenchSnapshot(current, incoming, options);
  if (mergedWorkbench === current) {
    return injectCachedSubagentSurfaceMessages(current, cache);
  }
  rememberSubagentSurfaceMessages(cache, mergedWorkbench);
  return injectCachedSubagentSurfaceMessages(mergedWorkbench, cache);
}

// mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache preserves child surfaces during focused hydration merges.
// mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache 在聚焦水合合并期间保留子代理界面。
//
// Parameters:
// 参数：
// current is the current workbench snapshot.
// current 是当前工作台快照。
// incoming is the conversation hydration snapshot returned by the service.
// incoming 是服务返回的会话水合快照。
// cache stores loaded subagent panel message windows.
// cache 保存已载入的子代理面板消息窗口。
// options controls local state preservation for fresh full-snapshot merges.
// options 控制较新完整快照合并时的本地状态保留策略。
//
// Returns:
// 返回值：
// A workbench snapshot with focused hydration data and cached subagent panels preserved.
// 返回保留聚焦水合数据与子代理面板缓存的工作台快照。
export function mergeConversationHydrationSnapshotWithSubagentSurfaceMessageCache(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
  options?: WorkbenchMergeOptions,
): WorkbenchSnapshot {
  const mergedWorkbench = mergeConversationHydrationSnapshot(current, incoming, options);
  if (mergedWorkbench === current) {
    return injectCachedSubagentSurfaceMessages(current, cache);
  }
  rememberSubagentSurfaceMessages(cache, mergedWorkbench);
  return injectCachedSubagentSurfaceMessages(mergedWorkbench, cache);
}

// mergeConversationMessagesWithSubagentSurfaceMessageCache replaces one message window and updates child-surface cache when applicable.
// mergeConversationMessagesWithSubagentSurfaceMessageCache 替换单个消息窗口，并在适用时更新子代理界面缓存。
export function mergeConversationMessagesWithSubagentSurfaceMessageCache(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  messages: ConversationMessage[],
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  if (workbench.subagentNavigation?.tasks?.some((task) => task.taskId === conversationId) || cache.has(conversationId)) {
    cache.set(conversationId, messages);
  }
  return injectCachedSubagentSurfaceMessages(mergeConversationMessages(workbench, conversationId, messages), cache);
}

// applySubagentPanelRouteResponse applies one focused subagent route response without merging a full snapshot.
// applySubagentPanelRouteResponse 应用单个聚焦子代理路由响应，不合并完整快照。
export function applySubagentPanelRouteResponse(
  workbench: WorkbenchSnapshot,
  response: SubagentPanelRouteResponse,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  const currentRevision = workbench.revision ?? 0;
  const responseRevision = response.revision ?? 0;
  if (currentRevision > 0 && responseRevision < currentRevision) {
    return workbench;
  }
  const activeConversationId = response.activeConversationId ?? response.subagentNavigation?.activeConversationId ?? null;
  const shouldCacheAsSubagentSurface = activeConversationId
    ? (response.subagentNavigation?.tasks?.some((task) => task.taskId === activeConversationId) ?? false)
      || cache.has(activeConversationId)
    : false;
  if (activeConversationId && shouldCacheAsSubagentSurface) {
    cache.set(activeConversationId, response.messages);
  }
  return injectCachedSubagentSurfaceMessages(deriveSelectedConversationFields({
    ...workbench,
    revision: response.revision,
    subagentNavigationByConversation: upsertSubagentNavigationForConversation(
      workbench.subagentNavigationByConversation ?? [],
      response.parentConversationId ?? response.subagentNavigation?.parentConversationId ?? null,
      response.subagentNavigation ?? null,
    ),
    messages: activeConversationId
      ? mergeConversationMessages(workbench, activeConversationId, response.messages).messages
      : workbench.messages,
  }), cache);
}

// applySessionAddonsChangedResponse applies one direct addon-state RPC response.
// applySessionAddonsChangedResponse 应用一次直接附属状态 RPC 响应。
export function applySessionAddonsChangedResponse(
  workbench: WorkbenchSnapshot,
  response: SessionAddonsChangedResponse,
): WorkbenchSnapshot {
  if (!shouldAcceptWorkbenchRevision(workbench.revision ?? 0, response.revision ?? 0)) {
    return workbench;
  }
  return deriveSelectedConversationFields({
    ...workbench,
    revision: response.revision,
    conversations: applySessionAddonAttention(workbench.conversations, response),
    sessionAddonsByConversation: upsertSessionAddonsForConversation(
      workbench.sessionAddonsByConversation ?? [],
      response,
    ),
    sessionVmmPoliciesByConversation: upsertSessionVmmPolicyForConversation(
      workbench.sessionVmmPoliciesByConversation ?? [],
      response,
    ),
  });
}

// rememberSubagentSurfaceMessages records message windows for current tasks and already opened cached child tabs.
// rememberSubagentSurfaceMessages 记录当前任务与已打开缓存子页签的消息窗口。
function rememberSubagentSurfaceMessages(cache: SubagentSurfaceMessageCache, workbench: WorkbenchSnapshot): void {
  const taskIds = new Set([
    ...(workbench.subagentNavigation?.tasks?.map((task) => task.taskId) ?? []),
    ...cache.keys(),
  ]);
  if (taskIds.size === 0) {
    return;
  }
  const messagesByTaskId = new Map<string, ConversationMessage[]>();
  for (const message of workbench.messages) {
    if (!taskIds.has(message.conversationId)) {
      continue;
    }
    const taskMessages = messagesByTaskId.get(message.conversationId) ?? [];
    taskMessages.push(message);
    messagesByTaskId.set(message.conversationId, taskMessages);
  }
  for (const [taskId, messages] of messagesByTaskId) {
    cache.set(taskId, messages);
  }
}

// injectCachedSubagentSurfaceMessages reattaches every cached child message window independent of the active floating list.
// injectCachedSubagentSurfaceMessages 重新挂回所有已缓存子代理消息窗口，不依赖当前浮动列表。
export function injectCachedSubagentSurfaceMessages(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  const taskIdsWithCache = new Set(cache.keys());
  if (taskIdsWithCache.size === 0) {
    return workbench;
  }
  const retainedMessages = workbench.messages.filter((message) => !taskIdsWithCache.has(message.conversationId));
  const cachedMessages = [...taskIdsWithCache].flatMap((taskId) => cache.get(taskId) ?? []);
  return {
    ...workbench,
    messages: [...retainedMessages, ...cachedMessages],
  };
}

// hasCachedSubagentSurfaceMessages returns whether one child transcript is already loaded in this frontend session.
// hasCachedSubagentSurfaceMessages 返回当前前端会话中是否已经载入指定子转录。
function hasCachedSubagentSurfaceMessages(cache: SubagentSurfaceMessageCache, taskId: string): boolean {
  return (cache.get(taskId)?.length ?? 0) > 0;
}

// canOpenCachedSubagentSurface verifies that cached messages and current navigation can satisfy a side-panel transcript open.
// canOpenCachedSubagentSurface 校验缓存消息与当前导航是否足以完成侧栏转录打开。
export function canOpenCachedSubagentSurface(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
  parentConversationId: string,
  taskId: string,
): boolean {
  return hasCachedSubagentSurfaceMessages(cache, taskId)
    && workbench.subagentNavigation?.parentConversationId === parentConversationId;
}

// mergeConversationSummaries preserves only optimistic local read state while trusting backend lifecycle state.
// mergeConversationSummaries 仅保留本地乐观已读状态，同时信任后端生命周期状态。
function mergeConversationSummaries(
  current: ConversationSummary[],
  incoming: ConversationSummary[],
  selectedConversationId: string | null | undefined,
): ConversationSummary[] {
  const currentById = new Map(current.map((conversation) => [conversation.id, conversation]));
  return incoming.map((conversation) => {
    const currentConversation = currentById.get(conversation.id);
    if (!currentConversation) {
      return conversation;
    }
    const keepUnread = conversation.id === selectedConversationId
      ? false
      : conversation.unread;
    return {
      ...conversation,
      unread: keepUnread,
    };
  });
}

// reorderPendingPromptSummaries applies an optimistic pending-prompt order by id.
// reorderPendingPromptSummaries 根据标识应用乐观待发送提示词排序。
export function reorderPendingPromptSummaries(entries: PendingPromptSummary[], entryIds: string[]): PendingPromptSummary[] {
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  const reordered = entryIds
    .map((entryId) => entriesById.get(entryId))
    .filter((entry): entry is PendingPromptSummary => Boolean(entry));
  const orderedIds = new Set(entryIds);
  return [
    ...reordered,
    ...entries.filter((entry) => !orderedIds.has(entry.id)),
  ];
}

// mergeConversationMessages replaces one conversation message window in the local workbench state.
// mergeConversationMessages 替换本地工作台状态中单个会话的消息窗口。
function mergeConversationMessages(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  messages: ConversationMessage[],
): WorkbenchSnapshot {
  return {
    ...workbench,
    messages: [
      ...workbench.messages.filter((message) => message.conversationId !== conversationId),
      ...messages,
    ],
  };
}

// markConversationLocallyRead clears the unread dot and records the selected conversation locally.
// markConversationLocallyRead 清除未读圆点，并在本地记录当前选中的会话。
export function markConversationLocallyRead(workbench: WorkbenchSnapshot, conversationId: string): WorkbenchSnapshot {
  return deriveSelectedConversationFields({
    ...workbench,
    selectedConversationId: conversationId,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === conversationId
        ? {
          ...conversation,
          unread: false,
          attention: conversation.attention?.kind === "unread" ? null : conversation.attention,
        }
        : conversation
    )),
  });
}
