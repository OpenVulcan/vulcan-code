import type { ConversationBlock, ConversationBlockKind } from "@/components/conversation/conversation-model";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { isSubagentHostKind } from "@/components/conversation/conversation-tool-metadata";
import type { TranslationKey } from "@/i18n-messages";

// isContextCompactionBlock detects host-provided context-compaction status rows.
// isContextCompactionBlock 检测宿主提供的上下文压缩状态行。
export function isContextCompactionBlock(block: ConversationBlock): boolean {
  return block.kind === "notice" && Boolean(block.compactionSectionId || block.compactionRole || block.compactionState);
}

// isMergeableToolBlock returns whether a block belongs to one tool execution row.
// isMergeableToolBlock 返回某个块是否属于同一工具执行行。
export function isMergeableToolBlock(block: ConversationBlock): block is ToolExecutionConversationBlock {
  if (block.kind === "tool-call" || block.kind === "tool-result" || block.kind === "command") {
    return blockHasToolExecutionEvidence(block);
  }
  return block.kind === "diff" && blockHasToolExecutionEvidence(block);
}

// blockTitle returns a localized title for one semantic transcript block.
// blockTitle 返回单个语义转录块的本地化标题。
export function blockTitle(kind: ConversationBlockKind, toolName?: string, t?: (key: TranslationKey) => string): string {
  if (kind === "tool-call") return toolName ? `${t ? t("conversation.block.toolCall") : "Tool call"} · ${toolName}` : t ? t("conversation.block.toolCall") : "Tool call";
  if (kind === "tool-result") return toolName ? `${t ? t("conversation.block.toolResult") : "Tool result"} · ${toolName}` : t ? t("conversation.block.toolResult") : "Tool result";
  if (kind === "command") return t ? t("conversation.block.command") : "Command session";
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return t ? t("conversation.block.thinking") : "Thinking process";
  if (kind === "turn-stats") return "Turn Stats";
  if (kind === "plan-artifact") return t ? t("conversation.block.plan") : "Plan";
  if (kind === "notice") return t ? t("conversation.block.notice") : "Run notice";
  return "Markdown";
}

// blockHasToolExecutionEvidence checks that a special block really belongs to a tool execution.
// blockHasToolExecutionEvidence 检查特殊块确实属于一次工具执行。
function blockHasToolExecutionEvidence(block: ConversationBlock): boolean {
  if (block.toolEvidence || block.toolCallId || block.commandSessionId || block.subagentTaskId) {
    return true;
  }
  const hostKind = metadataString(block.rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  return Boolean(hostKind && (
    isSubagentHostKind(hostKind)
    || ["tool_error", "tool_result", "tool_call", "command_session", "command_run", "change_set"].includes(hostKind)
  ));
}

// metadataString reads a dotted path string from unknown JSON metadata.
// metadataString 从未知 JSON 元数据中读取点分路径字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = metadataValue(metadata, [path]);
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataValue reads a dotted path value from unknown JSON metadata.
// metadataValue 从未知 JSON 元数据中读取点分路径值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  for (const path of paths) {
    const value = path.split(".").reduce<unknown>((current, segment) => {
      if (current && typeof current === "object" && segment in current) {
        return (current as Record<string, unknown>)[segment];
      }
      return undefined;
    }, metadata);
    if (value !== undefined && value !== null) {
      return value;
    }
  }
  return undefined;
}
