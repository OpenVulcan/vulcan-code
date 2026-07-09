import {
  conversationIsRunning,
  normalizedSessionRunPhase,
} from "@/components/conversation/conversation-runtime";
import { metadataString } from "@/components/conversation/conversation-transcript";
import type {
  AppendMessageBodyDelta,
  ConversationMessage,
  ConversationMessagesPatchedDelta,
  ConversationSummary,
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// workbenchDeltaConversationId returns the conversation touched by one partial update.
// workbenchDeltaConversationId 返回单条局部更新影响的会话标识。
export function workbenchDeltaConversationId(envelope: WorkbenchDeltaEnvelope): string | null {
  switch (envelope.delta.kind) {
    case "appendMessageBody":
      return envelope.delta.payload.conversationId;
    case "updateConversationRunPhase":
      return envelope.delta.payload.conversationId;
    case "pendingPromptsChanged":
      return envelope.delta.payload.conversationId;
    case "conversationSummaryChanged":
      return envelope.delta.payload.conversation.id;
    case "pendingQuestionChanged":
      return null;
    case "commandSessionsChanged":
      return null;
    case "subagentNavigationChanged":
      return envelope.delta.payload.parentConversationId
        ?? envelope.delta.payload.subagentNavigation?.parentConversationId
        ?? null;
    case "contextUsageChanged":
      return null;
    case "contextInjectionChanged":
      return envelope.delta.payload.conversationId;
    case "conversationMessagesChanged":
      return envelope.delta.payload.conversationId;
    case "conversationMessagesPatched":
      return envelope.delta.payload.conversationId;
    case "sessionAddonsChanged":
      return envelope.delta.payload.conversationId;
    case "conversationHydrationChanged":
      return envelope.delta.payload.progress.sessionId;
  }
}

// coalesceWorkbenchDeltaEnvelopes merges adjacent append-only chunks for the same message.
// coalesceWorkbenchDeltaEnvelopes 合并同一消息的相邻追加式片段。
export function coalesceWorkbenchDeltaEnvelopes(envelopes: WorkbenchDeltaEnvelope[]): WorkbenchDeltaEnvelope[] {
  const coalesced: WorkbenchDeltaEnvelope[] = [];
  for (const envelope of envelopes) {
    const previous = coalesced.at(-1);
    if (
      previous
      && previous.delta.kind === "appendMessageBody"
      && envelope.delta.kind === "appendMessageBody"
      && canCoalesceAppendMessageBodyDelta(previous, envelope)
    ) {
      const previousPayload = previous.delta.payload;
      const nextPayload = envelope.delta.payload;
      previous.generation = Math.max(previous.generation, envelope.generation);
      previousPayload.bodyDelta += nextPayload.bodyDelta;
      previousPayload.currentBody = nextPayload.currentBody ?? previousPayload.currentBody ?? null;
      if (nextPayload.initialMessage && (!previousPayload.initialMessage || nextPayload.initialMessage.body.length > previousPayload.initialMessage.body.length)) {
        previousPayload.initialMessage = { ...nextPayload.initialMessage };
      } else if (previousPayload.initialMessage) {
        previousPayload.initialMessage = {
          ...previousPayload.initialMessage,
          body: previousPayload.currentBody ?? appendStreamingTextDelta(previousPayload.initialMessage.body, nextPayload.bodyDelta),
        };
      }
      previousPayload.runPhase = nextPayload.runPhase ?? previousPayload.runPhase;
      continue;
    }
    coalesced.push(cloneWorkbenchDeltaEnvelope(envelope));
  }
  return coalesced;
}

// canCoalesceAppendMessageBodyDelta checks whether two envelopes target the same append-only row.
// canCoalesceAppendMessageBodyDelta 检查两个信封是否指向同一条追加式消息行。
function canCoalesceAppendMessageBodyDelta(previous: WorkbenchDeltaEnvelope, next: WorkbenchDeltaEnvelope): boolean {
  if (previous.delta.kind !== "appendMessageBody" || next.delta.kind !== "appendMessageBody") {
    return false;
  }
  return previous.delta.payload.conversationId === next.delta.payload.conversationId
    && previous.delta.payload.messageId === next.delta.payload.messageId;
}

// cloneWorkbenchDeltaEnvelope clones mutable delta envelopes before frame-level coalescing edits them.
// cloneWorkbenchDeltaEnvelope 在帧级合并修改信封前克隆可变增量数据。
function cloneWorkbenchDeltaEnvelope(envelope: WorkbenchDeltaEnvelope): WorkbenchDeltaEnvelope {
  if (envelope.delta.kind === "appendMessageBody") {
    return {
      generation: envelope.generation,
      delta: {
        kind: "appendMessageBody",
        payload: {
          ...envelope.delta.payload,
          initialMessage: envelope.delta.payload.initialMessage
            ? { ...envelope.delta.payload.initialMessage }
            : envelope.delta.payload.initialMessage,
        },
      },
    };
  }
  return envelope;
}

// applyAppendMessageBodyDelta appends streamed text to one local workbench message window.
// applyAppendMessageBodyDelta 将流式文本追加到本地工作台的单个消息窗口。
export function applyAppendMessageBodyDelta(
  workbench: WorkbenchSnapshot,
  delta: AppendMessageBodyDelta,
): WorkbenchSnapshot {
  const messages = appendMessageBodyDeltaToMessages(workbench.messages, delta);
  const runPhase = delta.runPhase ?? undefined;
  const subagentNavigationProjection = updateWorkbenchSubagentNavigationRunPhase(workbench, delta.conversationId, runPhase);
  return {
    ...workbench,
    conversations: workbench.conversations.map((conversation) => {
      if (conversation.id !== delta.conversationId) {
        return conversation;
      }
      return {
        ...conversation,
        runPhase: runPhase ?? conversation.runPhase,
        status: runPhase && !["idle", "ready", "failed", "stopped"].includes(normalizedSessionRunPhase(runPhase) ?? "")
          ? "Running"
          : conversation.status,
        lastUpdatedLabel: "now",
      };
    }),
    ...subagentNavigationProjection,
    messages,
  };
}

// applyConversationMessagesPatchedDelta applies node-level transcript updates without rebuilding unrelated messages.
// applyConversationMessagesPatchedDelta 应用节点级转录更新，避免重建无关消息。
export function applyConversationMessagesPatchedDelta(
  workbench: WorkbenchSnapshot,
  delta: ConversationMessagesPatchedDelta,
): WorkbenchSnapshot {
  const runPhase = delta.runPhase ?? undefined;
  const subagentNavigationProjection = updateWorkbenchSubagentNavigationRunPhase(workbench, delta.conversationId, runPhase);
  return {
    ...workbench,
    conversations: runPhase
      ? workbench.conversations.map((conversation) => {
        if (conversation.id !== delta.conversationId) {
          return conversation;
        }
        return {
          ...conversation,
          runPhase,
          status: !["idle", "ready", "failed", "stopped"].includes(normalizedSessionRunPhase(runPhase) ?? "")
            ? "Running"
            : conversation.status,
          lastUpdatedLabel: "now",
        };
      })
      : workbench.conversations,
    ...subagentNavigationProjection,
    messages: patchConversationMessages(workbench.messages, delta),
  };
}

// patchConversationMessages removes, upserts, and optionally reorders one conversation's visible nodes.
// patchConversationMessages 对单个会话的可见节点执行删除、写入和可选重排。
export function patchConversationMessages(
  messages: ConversationMessage[],
  delta: ConversationMessagesPatchedDelta,
): ConversationMessage[] {
  const patchedTargetMessages = buildPatchedConversationMessages(messages, delta);
  const firstTargetIndex = messages.findIndex((message) => message.conversationId === delta.conversationId);
  if (firstTargetIndex < 0) {
    return [...messages, ...patchedTargetMessages];
  }

  const nextMessages: ConversationMessage[] = [];
  let insertedTarget = false;
  for (const message of messages) {
    if (message.conversationId !== delta.conversationId) {
      nextMessages.push(message);
      continue;
    }
    if (!insertedTarget) {
      nextMessages.push(...patchedTargetMessages);
      insertedTarget = true;
    }
  }
  return nextMessages;
}

// buildPatchedConversationMessages builds the next visible message list for one conversation.
// buildPatchedConversationMessages 构建单个会话下一版可见消息列表。
function buildPatchedConversationMessages(
  messages: ConversationMessage[],
  delta: ConversationMessagesPatchedDelta,
): ConversationMessage[] {
  const removeMessageIds = new Set(delta.removeMessageIds);
  const upsertMessagesById = new Map(delta.upsertMessages.map((message) => [message.id, message]));
  const targetMessages: ConversationMessage[] = [];

  for (const message of messages) {
    if (message.conversationId !== delta.conversationId || removeMessageIds.has(message.id)) {
      continue;
    }
    const upsertMessage = upsertMessagesById.get(message.id);
    if (upsertMessage) {
      targetMessages.push(upsertMessage);
      upsertMessagesById.delete(message.id);
      continue;
    }
    targetMessages.push(message);
  }

  targetMessages.push(...upsertMessagesById.values());
  return applyConversationMessageOrder(targetMessages, delta.messageOrder ?? null);
}

// applyConversationMessageOrder treats backend order as the authoritative visible target window.
// applyConversationMessageOrder 将后端顺序视为目标可见窗口的权威结果。
function applyConversationMessageOrder(
  messages: ConversationMessage[],
  messageOrder: string[] | null,
): ConversationMessage[] {
  if (!messageOrder) {
    return messages;
  }

  const messagesById = new Map(messages.map((message) => [message.id, message]));
  const orderedMessages: ConversationMessage[] = [];
  const usedMessageIds = new Set<string>();
  for (const messageId of messageOrder) {
    const message = messagesById.get(messageId);
    if (!message || usedMessageIds.has(messageId)) {
      continue;
    }
    orderedMessages.push(message);
    usedMessageIds.add(messageId);
  }
  return orderedMessages;
}

// appendMessageBodyDeltaToMessages updates one message array without cloning unrelated message objects.
// appendMessageBodyDeltaToMessages 更新单个消息数组，并避免克隆无关消息对象。
function appendMessageBodyDeltaToMessages(
  messages: ConversationMessage[],
  delta: AppendMessageBodyDelta,
): ConversationMessage[] {
  let found = false;
  const nextMessages = messages.map((message) => {
    if (message.id !== delta.messageId) {
      return message;
    }
    found = true;
    return mergeAppendMessageBodyDelta(message, delta);
  });
  if (found || !delta.initialMessage) {
    return nextMessages;
  }

  const appendedInitialMessage = appendInitialMessageBodyDelta(delta.initialMessage, delta.bodyDelta);
  const replacementIndex = findAppendMessageBodyReplacementIndex(nextMessages, delta, appendedInitialMessage);
  if (replacementIndex >= 0) {
    return nextMessages.map((message, index) => (index === replacementIndex ? appendedInitialMessage : message));
  }
  return [...nextMessages, appendedInitialMessage];
}

// mergeAppendMessageBodyDelta applies a streamed append over an existing row or a fuller initial snapshot.
// mergeAppendMessageBodyDelta 将流式追加应用到已有行或更完整的初始快照上。
function mergeAppendMessageBodyDelta(
  message: ConversationMessage,
  delta: AppendMessageBodyDelta,
): ConversationMessage {
  if (typeof delta.currentBody === "string") {
    return {
      ...(delta.initialMessage && delta.initialMessage.body.length >= message.body.length ? delta.initialMessage : message),
      body: delta.currentBody,
    };
  }
  if (delta.initialMessage && delta.initialMessage.body.length > message.body.length) {
    return delta.initialMessage;
  }
  return {
    ...message,
    body: appendStreamingTextDelta(message.body, delta.bodyDelta),
  };
}

// appendInitialMessageBodyDelta returns the host-projected full row for a newly materialized stream.
// appendInitialMessageBodyDelta 返回宿主为新流式记录投影出的完整行。
function appendInitialMessageBodyDelta(message: ConversationMessage, bodyDelta: string): ConversationMessage {
  void bodyDelta;
  return {
    ...message,
    body: message.body,
  };
}

// findAppendMessageBodyReplacementIndex finds a live row that already represents the same streamed assistant output.
// findAppendMessageBodyReplacementIndex 查找已经代表同一条流式助手输出的实时行。
function findAppendMessageBodyReplacementIndex(
  messages: ConversationMessage[],
  delta: AppendMessageBodyDelta,
  candidate: ConversationMessage,
): number {
  if (!isAssistantOutputMessage(candidate)) {
    return -1;
  }
  let lastUserIndex = -1;
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (message?.conversationId === delta.conversationId && message.role === "user") {
      lastUserIndex = index;
    }
  }
  const stableIndex = findStableAppendMessageBodyReplacementIndex(messages, delta, candidate, lastUserIndex);
  if (stableIndex >= 0) {
    return stableIndex;
  }
  return -1;
}

// findStableAppendMessageBodyReplacementIndex prefers durable stream identifiers over body heuristics.
// findStableAppendMessageBodyReplacementIndex 优先使用持久流标识，而不是正文启发式。
function findStableAppendMessageBodyReplacementIndex(
  messages: ConversationMessage[],
  delta: AppendMessageBodyDelta,
  candidate: ConversationMessage,
  lastUserIndex: number,
): number {
  for (let index = messages.length - 1; index > lastUserIndex; index -= 1) {
    const message = messages[index];
    if (
      message?.conversationId === delta.conversationId
      && message.id !== candidate.id
      && isAssistantOutputMessage(message)
      && messagesShareStableStreamIdentity(candidate, message)
    ) {
      return index;
    }
  }
  return -1;
}

// messagesShareStableStreamIdentity checks metadata keys that identify the same streamed assistant row.
// messagesShareStableStreamIdentity 检查可识别同一条助手流式行的元数据键。
function messagesShareStableStreamIdentity(left: ConversationMessage, right: ConversationMessage): boolean {
  if (left.conversationId !== right.conversationId) {
    return false;
  }
  const sharedMetadataPaths = [
    "webui_projection_barrier_id",
    "webuiProjectionBarrierId",
    "stream_id",
    "streamId",
    "response_id",
    "responseId",
    "provider_response_id",
    "providerResponseId",
    "output_id",
    "outputId",
  ];
  for (const path of sharedMetadataPaths) {
    const leftValue = metadataString(left.metadata, [path]);
    const rightValue = metadataString(right.metadata, [path]);
    if (leftValue && rightValue && leftValue === rightValue) {
      return true;
    }
  }
  return false;
}

// isAssistantOutputMessage checks whether one message is a visible assistant prose row.
// isAssistantOutputMessage 检查单条消息是否为可见助手正文行。
function isAssistantOutputMessage(message: ConversationMessage): boolean {
  return message.role === "assistant"
    && normalizedRecordKind(message.recordKind) === "assistantoutput"
    && message.body.trim().length > 0;
}

// appendStreamingTextDelta appends provider deltas without guessing textual overlap.
// appendStreamingTextDelta 追加 Provider 增量文本，不猜测文本重叠。
function appendStreamingTextDelta(current: string, incoming: string): string {
  if (!incoming) {
    return current;
  }
  return `${current}${incoming}`;
}

// updateSubagentNavigationRunPhase mirrors delta progress into the active child-task badge.
// updateSubagentNavigationRunPhase 将增量进度同步到活动子任务徽标。
export function updateSubagentNavigationRunPhase(
  navigation: WorkbenchSnapshot["subagentNavigation"],
  conversationId: string,
  runPhase?: string,
): WorkbenchSnapshot["subagentNavigation"] {
  if (!navigation?.tasks?.some((task) => task.taskId === conversationId)) {
    return navigation;
  }
  const normalized = normalizedSessionRunPhase(runPhase);
  if (!normalized) {
    return navigation;
  }
  const running = !["completed", "failed", "idle", "ready", "stopped"].includes(normalized);
  const runState = running ? "running" : terminalSubagentRunStateFromPhase(normalized);
  return {
    ...navigation,
    tasks: navigation.tasks.map((task) => (
      task.taskId === conversationId
        ? { ...task, isRunning: running, runState }
        : task
    )),
  };
}

// updateWorkbenchSubagentNavigationRunPhase mirrors child run-phase changes into selected and cached navigation projections.
// updateWorkbenchSubagentNavigationRunPhase 将子任务运行阶段同步到当前与缓存的子代理导航投影。
export function updateWorkbenchSubagentNavigationRunPhase(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  runPhase?: string,
): Pick<WorkbenchSnapshot, "subagentNavigation" | "subagentNavigationByConversation"> {
  return {
    subagentNavigation: updateSubagentNavigationRunPhase(workbench.subagentNavigation, conversationId, runPhase),
    subagentNavigationByConversation: workbench.subagentNavigationByConversation?.map((navigation) => (
      updateSubagentNavigationRunPhase(navigation, conversationId, runPhase) ?? navigation
    )),
  };
}

// terminalSubagentRunStateFromPhase maps a known terminal run phase to the visible subagent state key.
// terminalSubagentRunStateFromPhase 将已知终态运行阶段映射为可见子代理状态键。
function terminalSubagentRunStateFromPhase(phase: string): string {
  if (phase === "failed" || phase === "stopped") {
    return phase;
  }
  return "completed";
}

// groupMessagesByConversation groups a flat message window by owning conversation.
// groupMessagesByConversation 按所属会话分组扁平消息窗口。
export function groupMessagesByConversation(messages: ConversationMessage[]): Map<string, ConversationMessage[]> {
  const grouped = new Map<string, ConversationMessage[]>();
  for (const message of messages) {
    const bucket = grouped.get(message.conversationId) ?? [];
    bucket.push(message);
    grouped.set(message.conversationId, bucket);
  }
  return grouped;
}

// mergeIncomingConversationMessages preserves a live streamed answer until the durable final answer appears.
// mergeIncomingConversationMessages 在持久化最终回答出现前保留实时流式回答。
export function mergeIncomingConversationMessages(
  currentMessages: ConversationMessage[],
  incomingMessages: ConversationMessage[],
  currentConversation: ConversationSummary | undefined,
  incomingConversation: ConversationSummary | undefined,
): ConversationMessage[] {
  const latestCurrentOutput = latestAssistantOutputAfterLastUser(currentMessages);
  if (!latestCurrentOutput) {
    return incomingMessages;
  }
  const incomingOutput = latestAssistantOutputAfterLastUser(incomingMessages);
  if (incomingOutput) {
    if (incomingOutput.conversationId === latestCurrentOutput.conversationId
      && incomingOutput.id === latestCurrentOutput.id
      && incomingOutput.body.length >= latestCurrentOutput.body.length) {
      return incomingMessages;
    }
    if (incomingOutput.conversationId === latestCurrentOutput.conversationId
      && incomingOutput.id === latestCurrentOutput.id
      && latestCurrentOutput.body.length > incomingOutput.body.length) {
      return replaceIncomingAssistantOutput(incomingMessages, incomingOutput.id, latestCurrentOutput);
    }
  }
  const incomingTerminal = incomingConversationHasTerminalStats(incomingMessages);
  const currentRunning = conversationIsRunning(currentConversation);
  const incomingRunning = conversationIsRunning(incomingConversation);
  if (!currentRunning && !incomingRunning && !incomingTerminal) {
    return incomingMessages;
  }
  return insertCarriedStreamingOutput(incomingMessages, latestCurrentOutput);
}

// latestAssistantOutputAfterLastUser returns the newest visible assistant output in the active user turn.
// latestAssistantOutputAfterLastUser 返回当前用户回合内最新的可见助手输出。
function latestAssistantOutputAfterLastUser(messages: ConversationMessage[]): ConversationMessage | null {
  const lastUserTime = Math.max(
    -1,
    ...messages
      .filter((message) => message.role === "user")
      .map((message) => conversationMessageCreatedAtValue(message)),
  );
  return [...messages]
    .reverse()
    .find((message) => (
      isAssistantOutputMessage(message)
      && conversationMessageCreatedAtValue(message) >= lastUserTime
    )) ?? null;
}

// incomingConversationHasTerminalStats detects whether the backend has already published turn completion records.
// incomingConversationHasTerminalStats 检测后端是否已经发布回合完成统计记录。
function incomingConversationHasTerminalStats(messages: ConversationMessage[]): boolean {
  return messages.some((message) => normalizedRecordKind(message.recordKind) === "turnstats");
}

// replaceIncomingAssistantOutput swaps a stale snapshot row for the newer streamed row without showing both.
// replaceIncomingAssistantOutput 用更新的流式行替换滞后的快照行，避免同时显示两份输出。
function replaceIncomingAssistantOutput(
  messages: ConversationMessage[],
  incomingOutputId: string,
  streamedOutput: ConversationMessage,
): ConversationMessage[] {
  let replaced = false;
  const nextMessages: ConversationMessage[] = [];
  for (const message of messages) {
    if (message.id === streamedOutput.id && message.id !== incomingOutputId) {
      continue;
    }
    if (message.id === incomingOutputId) {
      nextMessages.push(streamedOutput);
      replaced = true;
      continue;
    }
    nextMessages.push(message);
  }
  return replaced ? nextMessages : insertCarriedStreamingOutput(messages, streamedOutput);
}

// insertCarriedStreamingOutput inserts a carried streamed output before terminal-looking records.
// insertCarriedStreamingOutput 将保留的流式输出插入到终态记录之前。
function insertCarriedStreamingOutput(messages: ConversationMessage[], output: ConversationMessage): ConversationMessage[] {
  if (messages.some((message) => message.id === output.id)) {
    return messages;
  }
  const insertIndex = messages.findIndex((message) => {
    const recordKind = normalizedRecordKind(message.recordKind);
    return recordKind === "summaryoutput" || recordKind === "turnstats";
  });
  if (insertIndex < 0) {
    return [...messages, output];
  }
  return [...messages.slice(0, insertIndex), output, ...messages.slice(insertIndex)];
}

// normalizedRecordKind normalizes backend record kind text for semantic comparisons.
// normalizedRecordKind 规范化后端记录类型文本以便语义比较。
function normalizedRecordKind(recordKind?: string | null): string {
  return recordKind?.trim().toLowerCase().replace(/[-_]/g, "") ?? "";
}

// conversationMessageCreatedAtValue parses a message timestamp for turn-local ordering.
// conversationMessageCreatedAtValue 解析消息时间戳用于回合内排序。
function conversationMessageCreatedAtValue(message: ConversationMessage): number {
  const value = Number.parseInt(message.createdAt ?? "", 10);
  return Number.isFinite(value) ? value : -1;
}
