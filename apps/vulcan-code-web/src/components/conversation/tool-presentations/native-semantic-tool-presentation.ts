import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { TranslationKey } from "@/i18n";
import type {
  GenericToolMarkdownPresentation,
  NativeSemanticToolFamily,
  NativeSemanticToolId,
  NativeSemanticToolPresentation,
  ToolPresentationDisplayState,
  ToolPresentationResolveOptions,
} from "./types";
import {
  argumentsFromMetadata,
  compactHash,
  fixedHostPayload,
  fixedToolName,
  normalizeToolName,
} from "./presentation-value-readers";
import {
  modelExtensionToolMarkdown,
  modelExtensionToolTitleKey,
  modelExtensionToolValueMarkdown,
} from "./model-extension-tool-presentation";

// NativeSemanticToolConfig fixes one native tool to its business card family and localized title.
// NativeSemanticToolConfig 将单个原生工具固定到业务卡片族与本地化标题。
interface NativeSemanticToolConfig {
  // Business card family.
  // 业务卡片族。
  family: NativeSemanticToolFamily;
  // Human-readable title key.
  // 面向用户的标题键。
  titleKey: TranslationKey;
}

// NATIVE_SEMANTIC_TOOL_CONFIGS is the exact allowlist for native semantic cards.
// NATIVE_SEMANTIC_TOOL_CONFIGS 是原生语义卡片使用的精确允许列表。
const NATIVE_SEMANTIC_TOOL_CONFIGS = new Map<NativeSemanticToolId, NativeSemanticToolConfig>([
  ["todolist", { family: "todo", titleKey: "toolPresentation.native.todolist" }],
  ["get_goal", { family: "goal", titleKey: "toolPresentation.native.getGoal" }],
  ["create_goal", { family: "goal", titleKey: "toolPresentation.native.createGoal" }],
  ["update_goal", { family: "goal", titleKey: "toolPresentation.native.updateGoal" }],
  ["knowledge_search", { family: "knowledge", titleKey: "toolPresentation.native.knowledgeSearch" }],
  ["knowledge_browse", { family: "knowledge", titleKey: "toolPresentation.native.knowledgeBrowse" }],
  ["knowledge_read", { family: "knowledge", titleKey: "toolPresentation.native.knowledgeRead" }],
  ["skill", { family: "skill", titleKey: "toolPresentation.native.skill" }],
  ["vmm_memory_search", { family: "memory", titleKey: "toolPresentation.native.memorySearch" }],
  ["vmm_turn_details", { family: "memory", titleKey: "toolPresentation.native.turnDetails" }],
  ["vmm_memory_write", { family: "memory", titleKey: "toolPresentation.native.memoryWrite" }],
  ["vmm_memory_delete", { family: "memory", titleKey: "toolPresentation.native.memoryDelete" }],
  ["runtime_config", { family: "runtime_config", titleKey: "toolPresentation.native.runtimeConfig" }],
]);

// resolveNativeSemanticToolPresentation recognizes only the fixed native-tool allowlist.
// resolveNativeSemanticToolPresentation 仅识别固定的原生工具允许列表。
export function resolveNativeSemanticToolPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): NativeSemanticToolPresentation | null {
  // rawToolName comes only from normalized visible tool metadata.
  // rawToolName 仅来自已规范化的可见工具元数据。
  const rawToolName = fixedToolName(blocks);
  if (!rawToolName) {
    return null;
  }
  // toolId is the canonical comparison identity used by the exact allowlist.
  // toolId 是精确允许列表使用的规范比较身份。
  const toolId = normalizeToolName(rawToolName) as NativeSemanticToolId;
  // config exists only for one explicitly supported native semantic tool.
  // config 仅对明确支持的原生语义工具存在。
  const config = NATIVE_SEMANTIC_TOOL_CONFIGS.get(toolId);
  if (!config) {
    return null;
  }
  return {
    arguments: options.requestPreview ?? visibleToolArguments(blocks),
    detailRef,
    family: config.family,
    id: semanticToolPresentationId(blocks, toolId),
    kind: "native_semantic",
    result: visibleToolResult(blocks),
    state: displayState,
    titleKey: config.titleKey,
    toolId,
  };
}

// resolveGenericToolMarkdownPresentation builds the final inline fallback for one unknown tool.
// resolveGenericToolMarkdownPresentation 为未知工具构建最终的内联兜底卡片。
export function resolveGenericToolMarkdownPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
  options: ToolPresentationResolveOptions = {},
): GenericToolMarkdownPresentation {
  // toolName preserves the unknown raw identity shown only on the generic fallback card.
  // toolName 保留仅在通用兜底卡上展示的未知原始身份。
  const toolName = fixedToolName(blocks) ?? blocks.find((block) => block.title.trim())?.title ?? "unknown";
  // titleKey exists only for one exact AI model-extension tool contract.
  // titleKey 仅对一个精确 AI 模型扩展工具契约存在。
  const titleKey = modelExtensionToolTitleKey(toolName) ?? undefined;
  // inputValue comes from the active request preview or the persisted invocation arguments.
  // inputValue 来自活动请求预览或持久化调用参数。
  const inputValue = options.requestPreview ?? visibleToolArguments(blocks);
  return {
    detailRef,
    id: semanticToolPresentationId(blocks, `generic:${toolName}`),
    kind: "generic_tool_markdown",
    markdown: modelExtensionToolMarkdown(toolName, visibleToolMarkdown(blocks)),
    inputMarkdown: titleKey ? modelExtensionToolValueMarkdown(toolName, inputValue) : undefined,
    state: displayState,
    titleKey,
    toolName,
  };
}

// visibleToolArguments returns the exact arguments retained on the visible invocation record.
// visibleToolArguments 返回可见调用记录上保留的精确参数。
function visibleToolArguments(blocks: ToolExecutionConversationBlock[]): unknown {
  for (const block of blocks) {
    // value is the exact arguments document retained by the current visible block.
    // value 是当前可见块保留的精确参数文档。
    const value = argumentsFromMetadata(block.rawMetadata);
    if (value !== null && value !== undefined) {
      return value;
    }
  }
  return null;
}

// visibleToolResult returns structured host data before falling back to visible text.
// visibleToolResult 优先返回结构化宿主数据，随后回退到可见文本。
function visibleToolResult(blocks: ToolExecutionConversationBlock[]): unknown {
  return fixedHostPayload(blocks) ?? visibleToolMarkdown(blocks);
}

// visibleToolMarkdown joins only non-empty result bodies for inline display.
// visibleToolMarkdown 仅连接非空结果正文用于内联展示。
function visibleToolMarkdown(blocks: ToolExecutionConversationBlock[]): string {
  return blocks
    .filter((block) => block.kind === "tool-result" || block.kind === "markdown" || block.kind === "notice")
    .map((block) => block.body.trim())
    .filter(Boolean)
    .join("\n\n");
}

// semanticToolPresentationId builds a stable card id from the authoritative call identity.
// semanticToolPresentationId 根据权威调用身份构建稳定卡片标识。
function semanticToolPresentationId(blocks: ToolExecutionConversationBlock[], prefix: string): string {
  // identity prefers the stable tool-call id and falls back to the durable block id.
  // identity 优先使用稳定工具调用标识，并回退到持久块标识。
  const identity = blocks.map((block) => block.toolCallId ?? block.id).find(Boolean) ?? prefix;
  return `${prefix}:${compactHash(identity)}`;
}
