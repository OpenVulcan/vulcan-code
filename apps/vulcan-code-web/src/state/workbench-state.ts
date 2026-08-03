import {
  applyAppendMessageBodyDelta,
  applyConversationMessagesPatchedDelta,
  groupMessagesByConversation,
  mergeIncomingConversationMessages,
  updateWorkbenchSubagentNavigationRunPhase,
} from "@/state/workbench-message-merge";
import type {
  ConversationHydrationProgress,
  ConversationArchivedDelta,
  ContextInjectionChangedDelta,
  ConversationMessage,
  ConversationSummary,
  PendingPromptsChangedDelta,
  PendingQuestionSnapshot,
  PendingPromptSummary,
  ProjectRemovedDelta,
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

// SubagentSurfaceMessageCache stores loaded child transcript messages by child task conversation id.
// SubagentSurfaceMessageCache 按子任务会话标识保存已载入的子代理转录消息。
export type SubagentSurfaceMessageCache = Map<string, ConversationMessage[]>;

// WorkbenchMergeOptions tunes snapshot merge behavior for local interaction state.
// WorkbenchMergeOptions 调整快照合并时本地交互状态的处理方式。
export interface WorkbenchMergeOptions {
  // Whether a confirmed service-instance change may reset monotonically increasing revisions.
  // 已确认服务实例切换时，是否允许重置单调递增的版本。
  allowServiceRevisionReset?: boolean;
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
  if (
    !options.allowServiceRevisionReset
    && !shouldAcceptWorkbenchRevision(currentRevision, incomingRevision)
  ) {
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
  // liveParentConversationIds contains every active regular conversation allowed to own a navigation tree.
  // liveParentConversationIds 包含允许拥有导航树的全部活动常规会话。
  const liveParentConversationIds = new Set(workbench.conversations.map((conversation) => conversation.id));
  // subagentNavigationByConversation removes trees whose regular parent is no longer active.
  // subagentNavigationByConversation 删除常规父会话已不活动的导航树。
  const subagentNavigationByConversation = (workbench.subagentNavigationByConversation ?? [])
    .filter((navigation) => liveParentConversationIds.has(navigation.parentConversationId));
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
    ? subagentNavigationByConversation.find((navigation) => navigation.parentConversationId === selectedConversationId) ?? null
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
    subagentNavigationByConversation,
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
  // ownsIndependentLifecycleDomain identifies a delta whose domain revision is accepted separately from unrelated state.
  // ownsIndependentLifecycleDomain 标识其领域版本独立于无关状态接收的增量。
  const ownsIndependentLifecycleDomain = envelope.delta.kind === "conversationArchived"
    || envelope.delta.kind === "conversationsArchived"
    || envelope.delta.kind === "projectRemoved"
    || envelope.delta.kind === "agentSkillsCatalogChanged"
    || envelope.delta.kind === "luaSkillsCatalogChanged"
    || envelope.delta.kind === "subagentCatalogChanged"
    || envelope.delta.kind === "pluginCatalogChanged"
    || envelope.delta.kind === "projectSettingsChanged"
    || envelope.delta.kind === "luaSkillsManagedRuntimeProgressChanged";
  if (!ownsIndependentLifecycleDomain && envelope.generation <= (workbench.revision ?? 0)) {
    return workbench;
  }
  // nextWorkbench applies only fields explicitly owned by this delta kind.
  // nextWorkbench 仅应用该增量类型明确拥有的字段。
  const nextWorkbench = applyWorkbenchDeltaPayloadWithSubagentSurfaceMessageCache(workbench, envelope, cache);
  return {
    ...nextWorkbench,
    revision: Math.max(workbench.revision ?? 0, envelope.generation),
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
    case "conversationArchived":
      return applyConversationArchivedDelta(workbench, envelope.delta.payload, cache);
    case "conversationsArchived":
      return applyConversationsArchivedDelta(
        workbench,
        envelope.delta.payload.archivedConversations,
        cache,
      );
    case "projectRemoved":
      return applyProjectRemovedDelta(workbench, envelope.delta.payload, cache);
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
    case "agentSkillsCatalogChanged":
      return {
        ...workbench,
        agentSkills: envelope.delta.payload.agentSkills,
      };
    case "luaSkillsCatalogChanged":
      return {
        ...workbench,
        luaSkills: envelope.delta.payload.luaSkills,
        plugins: envelope.delta.payload.plugins,
      };
    case "subagentCatalogChanged":
      return {
        ...workbench,
        subagentCatalog: envelope.delta.payload.subagentCatalog,
      };
    case "pluginCatalogChanged":
      return {
        ...workbench,
        luaSkills: envelope.delta.payload.luaSkills,
        plugins: envelope.delta.payload.plugins,
        agentSkills: envelope.delta.payload.agentSkills,
        subagentCatalog: envelope.delta.payload.subagentCatalog,
      };
    case "projectSettingsChanged":
      // changedProjectId selects the exact project-owned revision map or the global revision pair.
      // changedProjectId 选择准确的项目级版本映射或全局版本对。
      const changedProjectId = envelope.delta.payload.projectId;
      // changedDomain identifies the only revision advanced by this focused delta.
      // changedDomain 标识当前聚焦增量唯一推进的版本。
      const changedDomain = envelope.delta.payload.domain;
      if (changedProjectId === null) {
        return {
          ...workbench,
          projectSettingsRevisions: {
            ...workbench.projectSettingsRevisions,
            global: {
              ...workbench.projectSettingsRevisions.global,
              [changedDomain]: envelope.generation,
            },
          },
        };
      }
      // currentProjectRevisions provides the complete initial pair for a project's first focused delta.
      // currentProjectRevisions 为项目首次聚焦增量提供完整的初始版本对。
      const currentProjectRevisions = workbench.projectSettingsRevisions.byProject[changedProjectId]
        ?? { vmm: 0, agentSkills: 0, codebaseMemory: 0 };
      return {
        ...workbench,
        projectSettingsRevisions: {
          ...workbench.projectSettingsRevisions,
          byProject: {
            ...workbench.projectSettingsRevisions.byProject,
            [changedProjectId]: {
              ...currentProjectRevisions,
              [changedDomain]: envelope.generation,
            },
          },
        },
      };
    case "luaSkillsManagedRuntimeProgressChanged":
      return {
        ...workbench,
        luaSkills: {
          ...workbench.luaSkills,
          managedRuntimeProgress: envelope.delta.payload.progress,
        },
      };
    case "mediaUploadProgressChanged":
      // Media progress is an ordered ephemeral event; its bytes live only in the overlay state.
      // 媒体进度是有序临时事件，字节数仅保留在浮层状态中。
      return workbench;
  }
}

// applyConversationArchivedDelta commits the archive-owned lifecycle domain without replacing unrelated newer state.
// applyConversationArchivedDelta 提交归档拥有的生命周期领域，且不替换无关的较新状态。
//
// Parameters: workbench is the current mixed-domain state, delta is the authoritative archive transition, and cache owns loaded child surfaces.
// 参数：workbench 是当前混合领域状态，delta 是权威归档转换，cache 持有已载入子界面。
// Returns: the workbench with the archived parent and every parent-owned retained projection removed atomically.
// 返回值：原子移除已归档父会话及其全部父级保留投影后的工作台。
function applyConversationArchivedDelta(
  workbench: WorkbenchSnapshot,
  delta: ConversationArchivedDelta,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  // conversationId is the exact lifecycle identity retired by the backend commit.
  // conversationId 是后端提交所淘汰的精确生命周期身份。
  const conversationId = delta.conversationId;
  // conversations is the active collection after removing the committed archive target.
  // conversations 是移除已提交归档目标后的活动集合。
  const conversations = workbench.conversations.filter((conversation) => conversation.id !== conversationId);
  // selectedConversationId accepts the backend choice only when it remains visible in the active collection.
  // selectedConversationId 仅在后端选择仍存在于活动集合时接受该选择。
  const selectedConversationId = visibleConversationId(conversations, delta.selectedConversationId)
    ?? conversations[0]?.id
    ?? null;
  // retainedArchivedConversations removes any stale copy before applying the authoritative archived projection.
  // retainedArchivedConversations 在应用权威归档投影前移除任何过期副本。
  const retainedArchivedConversations = (workbench.archivedConversations ?? [])
    .filter((conversation) => conversation.id !== conversationId);
  // archivedConversations mirrors whether the post-archive workbench exposes this session as restorable.
  // archivedConversations 镜像归档后工作台是否将该会话暴露为可恢复状态。
  const archivedConversations = delta.archivedConversation
    ? [delta.archivedConversation, ...retainedArchivedConversations]
    : retainedArchivedConversations;
  // nextWorkbench removes every retained projection whose owning parent lifecycle has ended.
  // nextWorkbench 移除所属父会话生命周期已结束的每个保留投影。
  const nextWorkbench = deriveSelectedConversationFields({
    ...workbench,
    conversations,
    archivedConversations,
    selectedConversationId,
    messages: workbench.messages.filter((message) => message.conversationId !== conversationId),
    hydratedConversationWindows: (workbench.hydratedConversationWindows ?? [])
      .filter((window) => window.sessionId !== conversationId),
    conversationHydration: workbench.conversationHydration?.sessionId === conversationId
      ? null
      : workbench.conversationHydration,
    pendingPromptsByConversation: (workbench.pendingPromptsByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    pendingQuestions: (workbench.pendingQuestions ?? [])
      .filter((question) => question.sessionId !== conversationId),
    sessionAddonsByConversation: (workbench.sessionAddonsByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    sessionVmmPoliciesByConversation: (workbench.sessionVmmPoliciesByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    contextInjectionByConversation: (workbench.contextInjectionByConversation ?? [])
      .filter((entry) => entry.conversationId !== conversationId),
    contextUsage: workbench.selectedConversationId === conversationId ? null : workbench.contextUsage,
  });
  return injectCachedSubagentSurfaceMessages(nextWorkbench, cache);
}

// applyConversationsArchivedDelta commits a target-owned archive transition for every requested conversation.
// applyConversationsArchivedDelta 为每个请求会话提交一条目标自有归档转换。
//
// Parameters: workbench is the current mixed-domain state, deltas are committed archive transitions, and cache owns loaded child surfaces.
// 参数：workbench 是当前混合领域状态，deltas 是已提交归档转换，cache 持有已载入子界面。
// Returns: the workbench after every archive transition has been applied idempotently.
// 返回值：幂等应用全部归档转换后的工作台。
function applyConversationsArchivedDelta(
  workbench: WorkbenchSnapshot,
  deltas: ConversationArchivedDelta[],
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  return deltas.reduce(
    (nextWorkbench, delta) => applyConversationArchivedDelta(nextWorkbench, delta, cache),
    workbench,
  );
}

// applyProjectRemovedDelta commits project membership and all child archive transitions atomically.
// applyProjectRemovedDelta 原子提交项目成员关系及全部子会话归档转换。
//
// Parameters: workbench is the current mixed-domain state, delta is the committed project removal, and cache owns loaded child surfaces.
// 参数：workbench 是当前混合领域状态，delta 是已提交项目移除，cache 持有已载入子界面。
// Returns: the workbench after project and child lifecycle state converge to the backend commit.
// 返回值：项目与子生命周期状态收敛到后端提交后的工作台。
function applyProjectRemovedDelta(
  workbench: WorkbenchSnapshot,
  delta: ProjectRemovedDelta,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  // archivedWorkbench removes only the child-owned retained projections named by the lifecycle payload.
  // archivedWorkbench 仅移除生命周期载荷点名的子会话自有保留投影。
  const archivedWorkbench = applyConversationsArchivedDelta(
    workbench,
    delta.archivedConversations,
    cache,
  );
  return {
    ...archivedWorkbench,
    projects: delta.projects,
    expandedProjectIds: delta.expandedProjectIds,
  };
}

// applyConversationSummaryChangedDelta upserts one sidebar summary and applies its owned selection without touching transcript rows.
// applyConversationSummaryChangedDelta 写入单个侧栏摘要及其自有选择，不触碰转录消息行。
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
    selectedConversationId: delta.selectedConversationId === undefined
      ? workbench.selectedConversationId
      : visibleConversationId(conversations, delta.selectedConversationId),
  };
  return deriveSelectedConversationFields({
    ...nextWorkbench,
    conversations: applyNormalizedConversationAttention(conversations, nextWorkbench),
  });
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
  // belongsToLiveTask prevents late deltas from reviving a closed or detached task cache.
  // belongsToLiveTask 防止迟到增量重新激活已关闭或已脱离的任务缓存。
  const belongsToLiveTask = liveSubagentTaskIds(workbench).has(conversationId);
  if (!belongsToLiveTask) {
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
  if (liveSubagentTaskIds(workbench).has(conversationId)) {
    cache.set(conversationId, messages);
  }
  return injectCachedSubagentSurfaceMessages(mergeConversationMessages(workbench, conversationId, messages), cache);
}

// applySubagentPanelRouteResponse applies one route response while preserving messages from successful concurrent tab loads.
// applySubagentPanelRouteResponse 应用单个路由响应，同时保留并发页签成功加载的消息。
// Parameters: workbench is the latest global snapshot; response is one route-local result; cache owns live child transcript windows.
// 参数：workbench 是最新全局快照；response 是单条路由局部结果；cache 持有存活子转录窗口。
// Returns: a full route projection for a current response, or a message-only merge when a newer global revision already won.
// 返回值：当前响应返回完整路由投影；较新全局版本已生效时仅合并消息。
export function applySubagentPanelRouteResponse(
  workbench: WorkbenchSnapshot,
  response: SubagentPanelRouteResponse,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  // activeConversationId identifies the route-local transcript independently from global navigation revision order.
  // activeConversationId 独立于全局导航版本顺序标识该路由的局部转录。
  const activeConversationId = response.activeConversationId ?? response.subagentNavigation?.activeConversationId ?? null;
  // currentRevision is the authoritative global projection revision already visible to the client.
  // currentRevision 是客户端当前可见的权威全局投影版本。
  const currentRevision = workbench.revision ?? 0;
  // responseRevision is the global projection revision captured while this route response was built.
  // responseRevision 是构建该路由响应时捕获的全局投影版本。
  const responseRevision = response.revision ?? 0;
  if (currentRevision > 0 && responseRevision < currentRevision) {
    // A stale global projection must never overwrite newer navigation, but its independently requested transcript remains valid
    // only while the latest navigation still declares that task live.
    // 过期全局投影绝不能覆盖较新导航；仅当最新导航仍声明该任务存活时，其独立请求的转录才仍然有效。
    if (!activeConversationId || !liveSubagentTaskIds(workbench).has(activeConversationId)) {
      return workbench;
    }
    return mergeConversationMessagesWithSubagentSurfaceMessageCache(
      workbench,
      activeConversationId,
      response.messages,
      cache,
    );
  }
  // shouldCacheAsSubagentSurface limits message retention to a child task declared by this accepted route projection.
  // shouldCacheAsSubagentSurface 将消息保留限制为本次已接受路由投影声明的子任务。
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

// liveSubagentTaskIds returns every unclosed task in active regular-conversation navigation trees.
// liveSubagentTaskIds 返回活动常规会话导航树中的全部未关闭任务。
// Parameters: workbench supplies active regular conversations and normalized navigation trees.
// 参数：workbench 提供活动常规会话与已归一化导航树。
// Returns: the authoritative live child-task id set, including nested tasks projected under each root.
// 返回值：权威存活子任务标识集合，包含每个根下投影的嵌套任务。
function liveSubagentTaskIds(workbench: WorkbenchSnapshot): Set<string> {
  // liveParentConversationIds limits navigation ownership to active regular conversations.
  // liveParentConversationIds 将导航所有权限制为活动常规会话。
  const liveParentConversationIds = new Set(workbench.conversations.map((conversation) => conversation.id));
  // navigationByParent lets the focused projection override its cached entry for the same parent.
  // navigationByParent 让聚焦投影覆盖同一父会话的缓存条目。
  const navigationByParent = new Map(
    (workbench.subagentNavigationByConversation ?? [])
      .map((navigation) => [navigation.parentConversationId, navigation] as const),
  );
  if (workbench.subagentNavigation) {
    navigationByParent.set(workbench.subagentNavigation.parentConversationId, workbench.subagentNavigation);
  }
  // taskIds accumulates the full projected tree for every live regular parent.
  // taskIds 累积每个存活常规父会话的完整投影树。
  const taskIds = new Set<string>();
  for (const [parentConversationId, navigation] of navigationByParent) {
    if (!liveParentConversationIds.has(parentConversationId)) {
      continue;
    }
    for (const task of navigation.tasks ?? []) {
      taskIds.add(task.taskId);
    }
  }
  return taskIds;
}

// rememberSubagentSurfaceMessages records message windows only for authoritative live child tasks.
// rememberSubagentSurfaceMessages 仅记录权威存活子任务的消息窗口。
// Parameters: cache owns retained child windows; workbench supplies current navigation and messages.
// 参数：cache 持有已保留子窗口；workbench 提供当前导航与消息。
// Returns: nothing; only live tasks with messages in the snapshot update the cache.
// 返回值：无；仅快照中带消息的存活任务更新缓存。
function rememberSubagentSurfaceMessages(cache: SubagentSurfaceMessageCache, workbench: WorkbenchSnapshot): void {
  const taskIds = liveSubagentTaskIds(workbench);
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

// injectCachedSubagentSurfaceMessages prunes retired tasks and reattaches every live cached child window.
// injectCachedSubagentSurfaceMessages 清理已退出任务，并重新挂回全部存活缓存子窗口。
// Parameters: workbench supplies authoritative live navigation; cache is pruned in place before message injection.
// 参数：workbench 提供权威存活导航；cache 在消息注入前就地清理。
// Returns: the original snapshot when no cache key changed messages, otherwise a snapshot with stale child rows removed and live cached rows attached.
// 返回值：无缓存键影响消息时返回原快照，否则返回已移除过期子行并挂载存活缓存行的快照。
export function injectCachedSubagentSurfaceMessages(
  workbench: WorkbenchSnapshot,
  cache: SubagentSurfaceMessageCache,
): WorkbenchSnapshot {
  // liveTaskIds is the authoritative retention boundary for cached child transcripts.
  // liveTaskIds 是缓存子转录的权威保留边界。
  const liveTaskIds = liveSubagentTaskIds(workbench);
  // retiredTaskIds retains removed cache keys long enough to strip their stale messages from the snapshot.
  // retiredTaskIds 临时保留已删除缓存键，以便从快照中移除其过期消息。
  const retiredTaskIds = new Set<string>();
  for (const taskId of cache.keys()) {
    if (liveTaskIds.has(taskId)) {
      continue;
    }
    retiredTaskIds.add(taskId);
    cache.delete(taskId);
  }
  const taskIdsWithCache = new Set(cache.keys());
  // taskIdsToReplace removes both live cached windows and just-retired stale windows from the base snapshot.
  // taskIdsToReplace 从基础快照中同时移除存活缓存窗口与刚退出的过期窗口。
  const taskIdsToReplace = new Set([...taskIdsWithCache, ...retiredTaskIds]);
  if (taskIdsToReplace.size === 0) {
    return workbench;
  }
  // retainedMessages contains every non-cached message from the authoritative snapshot.
  // retainedMessages 包含权威快照中的全部非缓存消息。
  const retainedMessages = workbench.messages.filter((message) => !taskIdsToReplace.has(message.conversationId));
  // cachedMessages flattens only the still-live child transcript windows.
  // cachedMessages 仅展平仍存活的子代理转录窗口。
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
