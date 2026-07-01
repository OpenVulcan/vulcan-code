import {
  applyAppendMessageBodyDelta,
  applyConversationMessagesPatchedDelta,
  groupMessagesByConversation,
  mergeIncomingConversationMessages,
  updateSubagentNavigationRunPhase,
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
}

// shouldAcceptWorkbenchRevision returns whether one incoming service revision can mutate UI state.
// shouldAcceptWorkbenchRevision 判断传入服务版本是否允许修改界面状态。
export function shouldAcceptWorkbenchRevision(currentRevision: number, incomingRevision: number): boolean {
  return currentRevision <= 0 || incomingRevision > currentRevision;
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
  const selectedConversationId = current.selectedConversationId
    && incoming.conversations.some((conversation) => conversation.id === current.selectedConversationId)
    ? current.selectedConversationId
    : incoming.selectedConversationId ?? incoming.conversations[0]?.id ?? null;
  const conversationHydration = selectMergedConversationHydration(
    current.conversationHydration ?? null,
    incoming.conversationHydration ?? null,
    selectedConversationId,
  );
  const mergedWorkbench = {
    ...incoming,
    selectedConversationId,
    expandedProjectIds,
    conversationHydration,
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
      deriveIncomingSubagentNavigations(incoming),
    ),
  };
  return deriveSelectedConversationFields({
    ...mergedWorkbench,
    conversations: applyNormalizedConversationAttention(mergedWorkbench.conversations, mergedWorkbench),
  });
}

// selectMergedConversationHydration keeps background conversation loads from overwriting the selected load progress.
// selectMergedConversationHydration 防止后台会话加载进度覆盖当前选中会话的加载进度。
function selectMergedConversationHydration(
  current: ConversationHydrationProgress | null,
  incoming: ConversationHydrationProgress | null,
  selectedConversationId: string | null,
): ConversationHydrationProgress | null {
  if (!incoming) {
    return current;
  }
  if (!selectedConversationId || incoming.sessionId === selectedConversationId) {
    return incoming;
  }
  if (current?.sessionId === selectedConversationId) {
    return current;
  }
  return incoming;
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
    contextInjection: contextInjectionEntry?.contextInjection ?? workbench.contextInjection ?? null,
  };
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

// contextInjectionForConversation returns the cached memory-panel projection for one conversation.
// contextInjectionForConversation 返回单个会话缓存的记忆面板投影。
function contextInjectionForConversation(
  workbench: WorkbenchSnapshot,
  conversationId: string,
): WorkbenchSnapshot["contextInjection"] {
  return workbench.contextInjectionByConversation
    ?.find((entry) => entry.conversationId === conversationId)
    ?.contextInjection ?? null;
}

// deriveIncomingSubagentNavigations extracts full-snapshot subagent navigation cache entries.
// deriveIncomingSubagentNavigations 提取全量快照中的子代理导航缓存条目。
function deriveIncomingSubagentNavigations(workbench: WorkbenchSnapshot): SubagentNavigationSnapshot[] {
  const entries = workbench.subagentNavigationByConversation ?? [];
  return workbench.subagentNavigation ? [...entries, workbench.subagentNavigation] : entries;
}

// mergeSubagentNavigationsByConversation merges subagent navigation snapshots by parent conversation id.
// mergeSubagentNavigationsByConversation 按父会话标识合并子代理导航快照。
function mergeSubagentNavigationsByConversation(
  current: SubagentNavigationSnapshot[],
  incoming: SubagentNavigationSnapshot[],
): SubagentNavigationSnapshot[] {
  const merged = new Map(current.map((navigation) => [navigation.parentConversationId, navigation]));
  for (const navigation of incoming) {
    merged.set(navigation.parentConversationId, navigation);
  }
  return [...merged.values()];
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
  return {
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
    subagentNavigation: updateSubagentNavigationRunPhase(
      workbench.subagentNavigation,
      delta.conversationId,
      delta.runPhase,
    ),
  };
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
      response.subagentNavigation?.parentConversationId ?? null,
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
    contextInjection: contextInjectionForConversation(workbench, conversationId),
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
