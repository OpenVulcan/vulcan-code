import {
  applyAppendMessageBodyDelta,
  groupMessagesByConversation,
  mergeIncomingConversationMessages,
} from "@/state/workbench-message-merge";
import type {
  ConversationHydrationProgress,
  ConversationMessage,
  ConversationSummary,
  PendingPromptSummary,
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
  promise: Promise<WorkbenchSnapshot | null>;
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

// mergeWorkbenchSnapshot merges backend workbench updates without discarding lazily loaded messages.
// mergeWorkbenchSnapshot 合并后端工作台更新且不丢弃已懒加载的会话正文。
export function mergeWorkbenchSnapshot(
  current: WorkbenchSnapshot,
  incoming: WorkbenchSnapshot,
  options: WorkbenchMergeOptions = {},
): WorkbenchSnapshot {
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
  const selectedConversationId = current.selectedConversationId ?? null;
  const conversationHydration = selectMergedConversationHydration(
    current.conversationHydration ?? null,
    incoming.conversationHydration ?? null,
    selectedConversationId,
  );
  return {
    ...incoming,
    selectedConversationId,
    expandedProjectIds,
    conversationHydration,
    conversations: mergeConversationSummaries(current.conversations, incoming.conversations, selectedConversationId),
    messages: [...retainedMessages, ...mergedIncomingMessages],
  };
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

// applyWorkbenchDeltaWithSubagentSurfaceMessageCache applies one partial update while keeping child transcript caches coherent.
// applyWorkbenchDeltaWithSubagentSurfaceMessageCache 应用单条局部更新，并保持子代理转录缓存一致。
export function applyWorkbenchDeltaWithSubagentSurfaceMessageCache(
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
  }
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
  rememberSubagentSurfaceMessages(cache, incoming);
  return injectCachedSubagentSurfaceMessages(mergeWorkbenchSnapshot(current, incoming, options), cache);
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
  return {
    ...workbench,
    selectedConversationId: conversationId,
    conversations: workbench.conversations.map((conversation) => (
      conversation.id === conversationId ? { ...conversation, unread: false } : conversation
    )),
  };
}
