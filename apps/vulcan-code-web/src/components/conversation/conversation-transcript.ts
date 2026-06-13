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
  const toolCallItems = new Map<string, ConversationRenderItem>();
  const compactionItems = new Map<string, ConversationRenderItem>();
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
      role: message.role === "system" ? "system" : "assistant",
      blocks: mergeContextCompactionBlocks(leadingThinkingBlocks, block),
      sourceMessage: message,
    };
    items.push(item);
    compactionItems.set(sectionId, item);
    return item;
  };

  for (const message of messages) {
    if (message.role === "user") {
      items.push({
        id: message.id,
        role: "user",
        blocks: [],
        sourceMessage: message,
        userMessage: message,
      });
      continue;
    }

    const blocks = parseConversationBlocks(message);
    for (const block of blocks) {
      if (isContextCompactionBlock(block)) {
        const sectionId = block.compactionSectionId ?? block.id;
        appendCompactionBlock(sectionId, block, message);
        if (block.compactionRole === "start") {
          activeCompactionSectionId = sectionId;
        }
        if (block.compactionRole === "end") {
          activeCompactionSectionId = activeCompactionSectionId === sectionId ? null : activeCompactionSectionId;
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
        const existing = block.kind !== "tool-call" ? findToolMergeItem(toolCallItems, block) : undefined;
        if (existing) {
          existing.blocks = mergeToolBlocks(existing.blocks, block);
          registerToolMergeItem(toolCallItems, existing);
          continue;
        }

        const item: ConversationRenderItem = {
          id: block.id,
          role: message.role === "system" ? "system" : "assistant",
          blocks: [block],
          sourceMessage: message,
        };
        items.push(item);
        registerToolMergeItem(toolCallItems, item);
        continue;
      }

      items.push({
        id: block.id,
        role: message.role === "system" ? "system" : "assistant",
        blocks: [block],
        sourceMessage: message,
      });
    }
  }

  return items;
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

// contextCompactionRoleOrder returns the display order for merged compaction rows.
// contextCompactionRoleOrder 返回合并压缩行的展示顺序。
function contextCompactionRoleOrder(role?: string): number {
  if (role === "start") return 0;
  if (role === "thinking") return 1;
  if (role === "body") return 2;
  if (role === "end") return 3;
  return 4;
}

// findToolMergeItem resolves an existing tool execution row using exact ids before the tool-name fallback.
// findToolMergeItem 优先使用精确标识，其次用工具名回退，解析已有工具执行行。
function findToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, block: ConversationBlock): ConversationRenderItem | undefined {
  for (const key of toolMergeKeysForBlock(block)) {
    const item = toolCallItems.get(key);
    if (item) {
      return item;
    }
  }
  return undefined;
}

// registerToolMergeItem indexes every known key for one visible tool execution row.
// registerToolMergeItem 为单条可见工具执行行登记所有已知合并键。
function registerToolMergeItem(toolCallItems: Map<string, ConversationRenderItem>, item: ConversationRenderItem): void {
  for (const block of item.blocks) {
    for (const key of toolMergeKeysForBlock(block)) {
      toolCallItems.set(key, item);
    }
  }
}

// toolMergeKeysForBlock resolves stable grouping keys from precise ids to the tool-name fallback.
// toolMergeKeysForBlock 按精确标识到工具名回退的顺序解析稳定分组键。
function toolMergeKeysForBlock(block: ConversationBlock): string[] {
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

