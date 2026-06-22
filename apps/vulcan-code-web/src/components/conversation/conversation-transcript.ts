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

export { generatedMediaAttachmentsFromToolDetailPayload } from "@/components/conversation/conversation-generated-media";
export { parseConversationBlocks } from "@/components/conversation/conversation-transcript-parser";
export { metadataString } from "@/components/conversation/conversation-metadata";

// projectConversationRenderItems merges related low-level transcript rows for display.
// projectConversationRenderItems 合并相关底层转录行以生成最终展示项。
export function projectConversationRenderItems(messages: ConversationMessage[]): ConversationRenderItem[] {
  const items: ConversationRenderItem[] = [];
  const toolMergeState = createToolMergeProjectionState();
  const compactionItems = new Map<string, ConversationRenderItem>();
  const projectionIds = conversationProjectionMessageIds(messages);
  const compactionSectionInstanceCounts = new Map<string, number>();
  const compactionSectionInstanceKeys = new Map<string, string>();
  let activeCompactionSectionId: string | null = null;

  const drainTrailingCompactionThinkingBlocks = (sectionId: string): ConversationBlock[] => {
    const collected: ConversationBlock[] = [];
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
    }
    return collected;
  };

  const appendCompactionBlock = (sectionId: string, block: ConversationBlock, message: ConversationMessage) => {
    const existing = compactionItems.get(sectionId);
    if (existing) {
      existing.blocks = mergeContextCompactionBlocks(existing.blocks, block);
      return existing;
    }
    const leadingThinkingBlocks = drainTrailingCompactionThinkingBlocks(sectionId);
    const item: ConversationRenderItem = {
      id: `context-compaction-${sectionId}`,
      renderKey: `context-compaction:${sectionId}`,
      role: message.role === "system" ? "system" : "assistant",
      blocks: mergeContextCompactionBlocks(leadingThinkingBlocks, block),
      sourceMessage: message,
    };
    items.push(item);
    compactionItems.set(sectionId, item);
    return item;
  };

  messages.forEach((message, messageIndex) => {
    const projectionId = projectionIds[messageIndex] ?? message.id;
    const projectionMessage = projectionId === message.id ? message : { ...message, id: projectionId };
    if (message.role === "user") {
      items.push({
        id: projectionId,
        renderKey: `item:${projectionId}`,
        role: "user",
        blocks: [],
        sourceMessage: message,
        userMessage: message,
      });
      return;
    }

    const blocks = parseConversationBlocks(projectionMessage);
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
          activeCompactionSectionId = activeCompactionSectionId === sectionKey ? null : activeCompactionSectionId;
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
          registerToolMergeItem(toolMergeState.itemsByKey, existing, mergeKeys);
          continue;
        }

        const item: ConversationRenderItem = {
          id: block.id,
          renderKey: `item:${block.id}`,
          role: message.role === "system" ? "system" : "assistant",
          blocks: [block],
          sourceMessage: message,
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
      });
    }
  });

  return repositionContextCompactionItems(items);
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

// thinkingOverlayBlockSignature returns a stable content key for delayed popup retention.
// thinkingOverlayBlockSignature 返回用于延迟保留思考弹窗的稳定内容键。
export function thinkingOverlayBlockSignature(block: ConversationBlock): string {
  return [
    block.id,
    block.title,
    block.body,
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

// repositionContextCompactionItems moves compaction markers only when the backend provides an explicit safe boundary.
// repositionContextCompactionItems 仅在后端提供明确安全边界时移动压缩标记。
function repositionContextCompactionItems(items: ConversationRenderItem[]): ConversationRenderItem[] {
  const next = [...items];
  for (const item of items) {
    if (!item.blocks.some(isContextCompactionBlock)) {
      continue;
    }
    const currentIndex = next.indexOf(item);
    if (currentIndex < 0) {
      continue;
    }
    const targetIndex = contextCompactionExplicitTargetIndex(next, item);
    if (targetIndex < 0 || currentIndex <= targetIndex) {
      continue;
    }
    next.splice(currentIndex, 1);
    next.splice(targetIndex, 0, item);
  }
  return next;
}

// contextCompactionExplicitTargetIndex finds a backend-provided visible insertion boundary.
// contextCompactionExplicitTargetIndex 查找后端提供的可见插入边界。
function contextCompactionExplicitTargetIndex(items: ConversationRenderItem[], item: ConversationRenderItem): number {
  const targetId = item.blocks
    .map((block) => block.compactionInsertBeforeMessageId)
    .find((value): value is string => Boolean(value));
  if (!targetId) {
    return -1;
  }
  const currentIndex = items.indexOf(item);
  if (currentIndex < 0) {
    return -1;
  }
  for (let index = currentIndex - 1; index >= 0; index -= 1) {
    const candidate = items[index];
    if (candidate && conversationRenderItemMessageId(candidate) === targetId) {
      return index;
    }
  }
  return -1;
}

// conversationRenderItemMessageId returns the stable visible transcript message id for one item.
// conversationRenderItemMessageId 返回单个展示项对应的稳定可见转录消息标识。
function conversationRenderItemMessageId(item: ConversationRenderItem): string | null {
  return item.userMessage?.id ?? item.sourceMessage?.id ?? null;
}

// contextCompactionRoleOrder returns the display order for merged compaction rows.
// contextCompactionRoleOrder 返回合并压缩行的展示顺序。
function contextCompactionRoleOrder(role?: string): number {
  if (role === "start") return 0;
  if (role === "thinking") return 1;
  if (role === "body") return 2;
  if (role === "end") return 3;
  return 4;
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

// toolMergeBaseKeysForBlock resolves stable grouping keys from precise ids to the tool-name fallback.
// toolMergeBaseKeysForBlock 按精确标识到工具名回退的顺序解析基础分组键。
function toolMergeBaseKeysForBlock(block: ConversationBlock): string[] {
  const keys = [
    block.toolCallId ? `call:${block.toolCallId}` : null,
    block.commandSessionId ? `command:${block.commandSessionId}` : null,
    block.toolResultId ? toolCallIdFromToolResultId(block.toolResultId) : null,
    block.toolName ? `name:${block.toolName}` : null,
  ];
  return keys.filter((key): key is string => Boolean(key));
}

// toolCallIdFromToolResultId recovers conventional tool-call ids from host-generated result ids.
// toolCallIdFromToolResultId 从宿主生成的结果标识中恢复约定形式的工具调用标识。
function toolCallIdFromToolResultId(toolResultId: string): string | null {
  const trimmed = toolResultId.trim();
  if (!trimmed.startsWith("tool-result-")) {
    return null;
  }
  const toolCallId = trimmed.slice("tool-result-".length).trim();
  return toolCallId ? `call:${toolCallId}` : null;
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
