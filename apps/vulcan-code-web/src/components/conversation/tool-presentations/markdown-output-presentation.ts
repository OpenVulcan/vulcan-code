import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { TranslationKey } from "@/i18n";
import type { MarkdownToolPresentation, MarkdownToolPresentationIcon, ToolPresentationDisplayState } from "./types";
import { compactHash, fixedToolName } from "./presentation-value-readers";

// MarkdownOutputToolConfig describes one exact tool id that should use the Markdown-output card.
// MarkdownOutputToolConfig 描述一个应使用 Markdown 输出卡片的精确工具标识。
interface MarkdownOutputToolConfig {
  // Stable id prefix used for expansion and cache keys.
  // 用于展开与缓存键的稳定标识前缀。
  idPrefix: string;
  // User-visible title translation key.
  // 用户可见标题的翻译键。
  titleKey: TranslationKey;
  // Icon slot used by the shared Markdown-output card.
  // 共享 Markdown 输出卡片使用的图标槽位。
  icon: MarkdownToolPresentationIcon;
}

// MARKDOWN_OUTPUT_TOOL_CONFIGS is keyed by concrete tool id, never by display text.
// MARKDOWN_OUTPUT_TOOL_CONFIGS 只按明确工具标识索引，绝不按展示文本匹配。
const MARKDOWN_OUTPUT_TOOL_CONFIGS = new Map<string, MarkdownOutputToolConfig>([
  ["vulcan_file_list", {
    icon: "folder_tree",
    idPrefix: "file-list",
    titleKey: "toolPresentation.fileList.title",
  }],
  ["vulcan_codekit_ast_tree", {
    icon: "ast_tree",
    idPrefix: "codekit-ast-tree",
    titleKey: "toolPresentation.codekit.astTree.title",
  }],
  ["vulcan_codekit_ast_detail", {
    icon: "ast_detail",
    idPrefix: "codekit-ast-detail",
    titleKey: "toolPresentation.codekit.astDetail.title",
  }],
  ["vulcan_codekit_rg", {
    icon: "search",
    idPrefix: "codekit-rg",
    titleKey: "toolPresentation.codekit.rg.title",
  }],
]);

// hydrateMarkdownToolPresentation merges a hidden tool detail payload into an existing Markdown-output presentation.
// hydrateMarkdownToolPresentation 将隐藏工具详情载荷合并到现有 Markdown 输出展示模型。
export function hydrateMarkdownToolPresentation(
  presentation: MarkdownToolPresentation,
  payload: ToolDetailPayload | null,
): MarkdownToolPresentation {
  if (!payload?.body.trim()) {
    return presentation;
  }
  return {
    ...presentation,
    markdown: payload.body,
  };
}

// resolveMarkdownToolPresentation recognizes only the configured concrete tool ids.
// resolveMarkdownToolPresentation 只识别已配置的明确工具标识。
export function resolveMarkdownToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): MarkdownToolPresentation | null {
  const toolId = fixedToolName(blocks);
  if (!toolId) {
    return null;
  }
  const config = MARKDOWN_OUTPUT_TOOL_CONFIGS.get(toolId);
  if (!config) {
    return null;
  }
  return {
    detailRef,
    icon: config.icon,
    id: markdownToolPresentationId(blocks, config.idPrefix),
    kind: "markdown_tool_output",
    markdown: visibleMarkdownToolOutput(blocks),
    state: displayState,
    titleKey: config.titleKey,
    toolId,
  };
}

// visibleMarkdownToolOutput returns any inline output already present on visible blocks.
// visibleMarkdownToolOutput 返回可见块中已经存在的内联输出。
function visibleMarkdownToolOutput(blocks: ToolExecutionConversationBlock[]): string {
  return blocks
    .map((block) => block.body.trim())
    .find(Boolean)
    ?? "";
}

// markdownToolPresentationId builds a stable id from the fixed tool call identity.
// markdownToolPresentationId 基于固定工具调用身份构建稳定标识。
function markdownToolPresentationId(blocks: ToolExecutionConversationBlock[], prefix: string): string {
  const identity = blocks
    .map((block) => block.toolCallId ?? block.toolResultId ?? block.id)
    .filter(Boolean)
    .join(":");
  return `${prefix}:${compactHash(identity || prefix)}`;
}
