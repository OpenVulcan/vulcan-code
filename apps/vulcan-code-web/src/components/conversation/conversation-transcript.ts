import type { ConversationMessage } from "@/types";
import {
  isContextCompactionBlock,
  isMergeableToolBlock,
} from "@/components/conversation/conversation-classifiers";
import type {
  ConversationBlock,
  ConversationBlockKind,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";

import { parseConversationBlocks } from "@/components/conversation/conversation-transcript-parser";
import type { TranslationKey } from "@/i18n";

export { generatedMediaAttachmentsFromToolDetailPayload } from "@/components/conversation/conversation-generated-media";
export { parseConversationBlocks } from "@/components/conversation/conversation-transcript-parser";
export { metadataString } from "@/components/conversation/conversation-metadata";

// projectConversationRenderItems merges related low-level transcript rows for display.
// projectConversationRenderItems 合并相关底层转录行以生成最终展示项。
export function projectConversationRenderItems(
  // Messages to project into render items.
  // 需要投影为渲染项的消息集合。
  messages: ConversationMessage[],
  // Optional translator used for localized metadata summaries.
  // 用于本地化元数据摘要的可选翻译函数。
  t?: (key: TranslationKey) => string,
): ConversationRenderItem[] {
  const items: ConversationRenderItem[] = [];
  const toolMergeState = createToolMergeProjectionState();
  const compactionItems = new Map<string, ConversationRenderItem>();
  const projectionIds = conversationProjectionMessageIds(messages);
  const compactionSectionInstanceCounts = new Map<string, number>();
  const compactionSectionInstanceKeys = new Map<string, string>();
  let activeCompactionSectionId: string | null = null;

  const drainTrailingCompactionThinkingBlocks = (sectionId: string): {
    blocks: ConversationBlock[];
    sourceMessages: ConversationMessage[];
  } => {
    const collected: ConversationBlock[] = [];
    const sourceMessages: ConversationMessage[] = [];
    while (items.length > 0) {
      const candidate = items[items.length - 1];
      if (!candidate || candidate.role !== "assistant" || !candidate.blocks.every((block) => block.kind === "thinking")) {
        break;
      }
      items.pop();
      collected.unshift(...candidate.blocks.map((block) => ({
        ...block,
        compactionSectionId: sectionId,
        compactionRole: "thinking",
      })));
      for (const sourceMessage of candidate.sourceMessages) {
        if (!sourceMessages.includes(sourceMessage)) {
          sourceMessages.push(sourceMessage);
        }
      }
    }
    return { blocks: collected, sourceMessages };
  };

  const appendCompactionBlock = (sectionId: string, block: ConversationBlock, message: ConversationMessage) => {
    const existing = compactionItems.get(sectionId);
    if (existing) {
      existing.blocks = mergeContextCompactionBlocks(existing.blocks, block);
      existing.sourceMessages = appendConversationRenderItemSourceMessage(existing.sourceMessages, message);
      return existing;
    }
    const leadingThinking = drainTrailingCompactionThinkingBlocks(sectionId);
    const item: ConversationRenderItem = {
      id: `context-compaction-${sectionId}`,
      renderKey: `context-compaction:${sectionId}`,
      role: message.role === "system" ? "system" : "assistant",
      blocks: mergeContextCompactionBlocks(leadingThinking.blocks, block),
      sourceMessage: message,
      sourceMessages: appendConversationRenderItemSourceMessage(leadingThinking.sourceMessages, message),
    };
    items.push(item);
    compactionItems.set(sectionId, item);
    return item;
  };

  messages.forEach((message, messageIndex) => {
    const projectionId = projectionIds[messageIndex] ?? message.id;
    const projectionMessage = projectionId === message.id ? message : { ...message, id: projectionId };
    if (message.role === "user") {
      activeCompactionSectionId = null;
      items.push({
        id: projectionId,
        renderKey: `item:${projectionId}`,
        role: "user",
        blocks: [],
        sourceMessage: message,
        sourceMessages: [message],
        userMessage: message,
      });
      return;
    }

    // blocks retain the backend message identity even when projectionId is rewritten for duplicate visual rows.
    // 即使 projectionId 因重复视觉行被改写，blocks 仍保留后端消息身份。
    const blocks = parseConversationBlocks(projectionMessage, t).map((block) => ({
      ...block,
      sourceMessageId: message.id,
    }));
    for (const block of blocks) {
      if (isContextCompactionBlock(block)) {
        const sectionId = block.compactionSectionId ?? block.id;
        const sectionKey = contextCompactionInstanceKey(
          sectionId,
          block.compactionRole,
          compactionSectionInstanceCounts,
          compactionSectionInstanceKeys,
        );
        appendCompactionBlock(sectionKey, block, message);
        if (block.compactionRole === "start") {
          activeCompactionSectionId = sectionKey;
        }
        if (block.compactionRole === "end") {
          activeCompactionSectionId = null;
        }
        continue;
      }

      if (activeCompactionSectionId && block.kind === "thinking") {
        appendCompactionBlock(activeCompactionSectionId, {
          ...block,
          compactionSectionId: activeCompactionSectionId,
          compactionRole: "thinking",
        }, message);
        continue;
      }

      if (isMergeableToolBlock(block)) {
        const mergeKeys = toolMergeInstanceKeysForBlock(toolMergeState, block);
        const existing = block.kind !== "tool-call" ? findToolMergeItem(toolMergeState.itemsByKey, mergeKeys) : undefined;
        if (existing) {
          existing.blocks = mergeToolBlocks(existing.blocks, block);
          existing.sourceMessages = appendConversationRenderItemSourceMessage(existing.sourceMessages, message);
          registerToolMergeItem(toolMergeState.itemsByKey, existing, mergeKeys);
          continue;
        }

        const item: ConversationRenderItem = {
          id: block.id,
          renderKey: `item:${block.id}`,
          role: message.role === "system" ? "system" : "assistant",
          blocks: [block],
          sourceMessage: message,
          sourceMessages: [message],
        };
        items.push(item);
        registerToolMergeItem(toolMergeState.itemsByKey, item, mergeKeys);
        continue;
      }

      items.push({
        id: block.id,
        renderKey: `item:${block.id}`,
        role: message.role === "system" ? "system" : "assistant",
        blocks: [block],
        sourceMessage: message,
        sourceMessages: [message],
      });
    }
  });

  return items;
}

// appendConversationRenderItemSourceMessage appends one exact source without duplicating its immutable reference.
// appendConversationRenderItemSourceMessage 追加一个精确来源，同时避免重复保存其不可变引用。
// Parameters: current contains sources already owned by the row; message is the source contributing the next block.
// 参数：current 包含该行已拥有的来源；message 是为下一个块贡献内容的来源。
// Returns: the unchanged source list when already present, otherwise a new ordered source list.
// 返回值：来源已存在时返回原集合，否则返回新的有序来源集合。
function appendConversationRenderItemSourceMessage(
  current: readonly ConversationMessage[],
  message: ConversationMessage,
): readonly ConversationMessage[] {
  if (current.length === 0) {
    return [message];
  }
  if (current.includes(message)) {
    return current;
  }
  return [...current, message];
}

// ToolMergeProjectionState tracks tool-call/result grouping per repeated historical execution instance.
// ToolMergeProjectionState 按重复历史执行实例跟踪工具调用与返回的分组状态。
interface ToolMergeProjectionState {
  // Visible tool rows keyed by instance-scoped merge keys.
  // 按实例作用域合并键索引的可见工具行。
  itemsByKey: Map<string, ConversationRenderItem>;
  // Latest execution instance id for each base tool merge key.
  // 每个基础工具合并键对应的最新执行实例标识。
  latestInstanceByBaseKey: Map<string, string>;
  // Execution instance counters keyed by primary base merge key.
  // 按主基础合并键记录的执行实例计数器。
  instanceCountsByPrimaryKey: Map<string, number>;
}

// createToolMergeProjectionState creates isolated state for one transcript projection pass.
// createToolMergeProjectionState 为单次转录投影创建隔离状态。
function createToolMergeProjectionState(): ToolMergeProjectionState {
  return {
    itemsByKey: new Map(),
    latestInstanceByBaseKey: new Map(),
    instanceCountsByPrimaryKey: new Map(),
  };
}

// conversationProjectionMessageIds returns per-message ids that keep historical duplicate rows render-safe.
// conversationProjectionMessageIds 返回让历史重复行安全渲染的逐消息标识。
function conversationProjectionMessageIds(messages: ConversationMessage[]): string[] {
  const totals = new Map<string, number>();
  for (const message of messages) {
    totals.set(message.id, (totals.get(message.id) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  return messages.map((message, index) => {
    const total = totals.get(message.id) ?? 0;
    if (total <= 1) {
      return message.id;
    }
    const nextSeen = (seen.get(message.id) ?? 0) + 1;
    seen.set(message.id, nextSeen);
    if (nextSeen === total) {
      return message.id;
    }
    const timestamp = message.createdAt?.trim() || "unknown";
    return `${message.id}::history-${timestamp}-${index}`;
  });
}

// contextCompactionInstanceKey separates repeated compaction sections from rollback history.
// contextCompactionInstanceKey 将回滚历史中重复出现的压缩分段隔离为不同实例。
function contextCompactionInstanceKey(
  sectionId: string,
  role: string | undefined,
  counts: Map<string, number>,
  latestKeys: Map<string, string>,
): string {
  if (role === "start" || !latestKeys.has(sectionId)) {
    const count = (counts.get(sectionId) ?? 0) + 1;
    counts.set(sectionId, count);
    const key = count === 1 ? sectionId : `${sectionId}::history-${count}`;
    latestKeys.set(sectionId, key);
    return key;
  }
  const key = latestKeys.get(sectionId) ?? sectionId;
  if (role === "end") {
    latestKeys.delete(sectionId);
  }
  return key;
}

// thinkingOverlayBlockSignature returns a stable structural key without rescanning cumulative stream text.
// thinkingOverlayBlockSignature 返回不重复扫描累计流文本的稳定结构键。
export function thinkingOverlayBlockSignature(block: ConversationBlock): string {
  return [
    block.id,
    block.title,
    block.summary,
    block.status ?? "",
  ].join("\u0000");
}

// mergeContextCompactionBlocks keeps one start/body/end order for a compaction section.
// mergeContextCompactionBlocks 为单个压缩分段保持 start/body/end 顺序。
function mergeContextCompactionBlocks(blocks: ConversationBlock[], block: ConversationBlock): ConversationBlock[] {
  const next = blocks.filter((candidate) => candidate.id !== block.id);
  next.push(block);
  return next.sort((left, right) => contextCompactionRoleOrder(left.compactionRole) - contextCompactionRoleOrder(right.compactionRole));
}

// contextCompactionRoleOrder returns the display order for merged compaction rows.
// contextCompactionRoleOrder 返回合并压缩行的展示顺序。
function contextCompactionRoleOrder(role?: string): number {
  if (role === "start") return 0;
  if (role === "progress") return 1;
  if (role === "thinking") return 2;
  if (role === "body") return 3;
  if (role === "end") return 4;
  return 5;
}

// findToolMergeItem resolves an existing tool execution row from instance-scoped merge keys.
// findToolMergeItem 通过实例作用域合并键解析已有工具执行行。
function findToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, keys: string[]): ConversationRenderItem | undefined {
  for (const key of keys) {
    const item = toolCallItems.get(key);
    if (item) {
      return item;
    }
  }
  return undefined;
}

// registerToolMergeItem indexes instance-scoped keys for one visible tool execution row.
// registerToolMergeItem 为单条可见工具执行行登记实例作用域合并键。
function registerToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, item: ConversationRenderItem, keys: string[]): void {
  for (const key of keys) {
    toolCallItems.set(key, item);
  }
}

// toolMergeInstanceKeysForBlock resolves merge keys scoped to one sequential tool execution instance.
// toolMergeInstanceKeysForBlock 解析限定在单个顺序工具执行实例内的合并键。
function toolMergeInstanceKeysForBlock(state: ToolMergeProjectionState, block: ConversationBlock): string[] {
  const baseKeys = toolMergeBaseKeysForBlock(block);
  if (baseKeys.length === 0) {
    return [];
  }
  const instanceKey = block.kind === "tool-call"
    ? createToolMergeInstanceKey(state, baseKeys)
    : latestToolMergeInstanceKey(state, baseKeys);
  for (const baseKey of baseKeys) {
    state.latestInstanceByBaseKey.set(baseKey, instanceKey);
  }
  return baseKeys.map((baseKey) => `${baseKey}@${instanceKey}`);
}

// createToolMergeInstanceKey starts a new execution instance for a tool-call block.
// createToolMergeInstanceKey 为工具调用块启动一个新的执行实例。
function createToolMergeInstanceKey(state: ToolMergeProjectionState, baseKeys: string[]): string {
  const primaryKey = baseKeys[0] ?? "tool";
  const nextCount = (state.instanceCountsByPrimaryKey.get(primaryKey) ?? 0) + 1;
  state.instanceCountsByPrimaryKey.set(primaryKey, nextCount);
  return `${primaryKey}#${nextCount}`;
}

// latestToolMergeInstanceKey reuses the nearest preceding call instance or creates a result-only one.
// latestToolMergeInstanceKey 复用最近的前置调用实例，或创建仅返回实例。
function latestToolMergeInstanceKey(state: ToolMergeProjectionState, baseKeys: string[]): string {
  for (const baseKey of baseKeys) {
    const instanceKey = state.latestInstanceByBaseKey.get(baseKey);
    if (instanceKey) {
      return instanceKey;
    }
  }
  return createToolMergeInstanceKey(state, baseKeys);
}

// toolMergeBaseKeysForBlock resolves stable grouping keys from precise ids.
// toolMergeBaseKeysForBlock 按精确标识解析稳定分组键。
function toolMergeBaseKeysForBlock(block: ConversationBlock): string[] {
  const keys = [
    block.toolCallId ? `call:${block.toolCallId}` : null,
    block.commandSessionId ? `command:${block.commandSessionId}` : null,
  ];
  return keys.filter((key): key is string => Boolean(key));
}

// mergeToolBlocks merges one tool result into an existing tool execution row.
// mergeToolBlocks 将一个工具返回合并到已有工具执行行中。
function mergeToolBlocks(blocks: ConversationBlock[], incoming: ConversationBlock): ConversationBlock[] {
  const withoutSameKind = blocks.filter((block) => block.kind !== incoming.kind);
  return [...withoutSameKind, incoming].sort((left, right) => toolBlockOrder(left.kind) - toolBlockOrder(right.kind));
}

// toolBlockOrder returns stable order for call/result display and detail assembly.
// toolBlockOrder 返回调用与返回展示及详情组装的稳定顺序。
function toolBlockOrder(kind: ConversationBlockKind): number {
  if (kind === "tool-call") return 0;
  if (kind === "command") return 1;
  if (kind === "tool-result" || kind === "diff") return 2;
  return 3;
}
