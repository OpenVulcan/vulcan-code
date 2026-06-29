import type { ConversationMessage } from "@/types";
import { blockTitle } from "@/components/conversation/conversation-classifiers";
import { generatedMediaAttachmentsFromMetadata } from "@/components/conversation/conversation-generated-media";
import {
  metadataString,
  metadataValue,
} from "@/components/conversation/conversation-metadata";
import type {
  ConversationBlock,
  ConversationBlockKind,
} from "@/components/conversation/conversation-model";
import {
  conversationBlockHasToolEvidence,
  extractToolResultIdFromText,
  metadataToolFailureStatus,
  normalizeToolDisplayState,
  structuredKindLooksLikeToolBlock,
  structuredMessageHasToolEvidence,
  subagentTaskIdFromToolMetadata,
  subagentToolDisplayName,
  toolMetadataContainsReturnPayload,
  turnStatsStatus,
  turnStatsSummary,
} from "@/components/conversation/conversation-tool-metadata";
import type { TranslationKey } from "@/i18n";

// directiveKinds maps Vulcan directive names to WebUI conversation block kinds.
// directiveKinds 将 Vulcan 指令名称映射为 WebUI 会话块类型。
const directiveKinds: Record<string, ConversationBlockKind> = {
  "tool-call": "tool-call",
  "tool-result": "tool-result",
  "turn-stats": "turn-stats",
  thinking: "thinking",
  diff: "diff",
  notice: "notice",
};

// parseConversationBlocks converts legacy transcript text into renderable WebUI blocks.
// parseConversationBlocks 将旧转录文本转换为可渲染的 WebUI 块。
export function parseConversationBlocks(
  // Source transcript message.
  // 源转录消息。
  message: ConversationMessage,
  // Optional translator used for localized metadata summaries.
  // 用于本地化元数据摘要的可选翻译函数。
  t?: (key: TranslationKey) => string,
): ConversationBlock[] {
  if (message.role === "user") {
    return [conversationMarkdownBlock(message.id, 0, message.body)];
  }

  const structuredKind = structuredMessageBlockKind(message);
  if (structuredKind) {
    const attributes = structuredMessageAttributes(message, t);
    if (structuredKindLooksLikeToolBlock(structuredKind) && !structuredMessageHasToolEvidence(message, attributes)) {
      return [conversationMarkdownBlock(message.id, 0, message.body)];
    }
    const primaryBlock = conversationSpecialBlock(message.id, 0, structuredKind, message.body, attributes, message.metadata);
    const planArtifact = planArtifactAttributesFromMetadata(message.metadata);
    if (!planArtifact || structuredKind === "plan-artifact") {
      return [primaryBlock];
    }
    return [
      primaryBlock,
      conversationSpecialBlock(
        message.id,
        1,
        "plan-artifact",
        planArtifact.content,
        {
          plan_name: planArtifact.name,
          plan_path: planArtifact.path,
          plan_content: planArtifact.content,
          plan_updated_at: planArtifact.updatedAt,
        },
        message.metadata,
      ),
    ];
  }

  const source = message.body.replace(/\r\n/g, "\n");
  const directiveBlocks = parseDirectiveConversationBlocks(message.id, source);
  if (directiveBlocks.some((block) => block.kind !== "markdown")) {
    return directiveBlocks;
  }

  return parseFallbackConversationBlocks(message.id, source);
}

// structuredMessageBlockKind maps service-provided record kinds to WebUI block kinds.
// structuredMessageBlockKind 将服务提供的记录类型映射到 WebUI 块类型。
function structuredMessageBlockKind(message: ConversationMessage): ConversationBlockKind | null {
  if (message.recordKind === "tool-call-output") {
    return "tool-call";
  }
  if (message.recordKind === "tool-result-output") {
    const hostKind = metadataString(message.metadata, ["host_result.kind", "hostResult.kind", "kind"]);
    if (hostKind === "command_session") return "command";
    if (hostKind === "change_set") return "diff";
    return "tool-result";
  }
  if (message.recordKind === "thinking-output") return "thinking";
  if (message.recordKind === "turn-stats") return "turn-stats";
  if (message.recordKind === "compaction-status") return "notice";
  return null;
}

// structuredMessageAttributes extracts render metadata from the service payload.
// structuredMessageAttributes 从服务载荷中提取渲染元数据。
function structuredMessageAttributes(
  // Source transcript message.
  // 源转录消息。
  message: ConversationMessage,
  // Optional translator used for localized turn statistics.
  // 用于本地化轮次统计的可选翻译函数。
  t?: (key: TranslationKey) => string,
): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolName = metadataString(message.metadata, [
    "tool_name",
    "host_result.tool_name",
    "hostResult.toolName",
  ]);
  const toolCallId = metadataString(message.metadata, [
    "tool_call_id",
    "host_result.tool_call_id",
    "hostResult.toolCallId",
  ]);
  const toolResultId = metadataString(message.metadata, [
    "tool_result_id",
    "host_result.tool_result_id",
    "hostResult.toolResultId",
  ]);
  const commandSessionId = metadataString(message.metadata, [
    "command_session_id",
    "host_result.command_session_id",
    "hostResult.commandSessionId",
    "host_result.payload.command_session_id",
    "hostResult.payload.commandSessionId",
    "payload.command_session_id",
    "payload.commandSessionId",
  ]);
  const subagentTaskId = subagentTaskIdFromToolMetadata(message.metadata);
  const subagentName = subagentToolDisplayName(message.metadata, toolName);
  const planArtifact = planArtifactAttributesFromMetadata(message.metadata);
  if (planArtifact) {
    attributes.plan_name = planArtifact.name;
    attributes.plan_path = planArtifact.path;
    attributes.plan_content = planArtifact.content;
    attributes.plan_updated_at = planArtifact.updatedAt;
  }
  const subagentTaskStatus = subagentTaskId ? metadataString(message.metadata, [
      "tool_display_state",
      "display_state",
      "host_result.payload.execution_status",
      "hostResult.payload.executionStatus",
    ]) : null;
  const explicitStatus = metadataString(message.metadata, [
      "result.status",
      "tool_display_state",
      "display_state",
      "host_result.payload.execution_status",
      "hostResult.payload.executionStatus",
      "status",
      "host_result.status",
      "hostResult.status",
    ]);
  const hasReturnedToolPayload = message.recordKind === "tool-result-output"
    && toolMetadataContainsReturnPayload(message.metadata, message.body);
  const fallbackStatus = hasReturnedToolPayload
    ? "completed"
    : explicitStatus
      ?? metadataToolFailureStatus(message.metadata)
      ?? turnStatsStatus(message.metadata);
  const status = subagentTaskStatus ?? fallbackStatus;
  const summary = metadataString(message.metadata, [
      "result.summary",
      "preview",
      "summary",
    ]) ?? turnStatsSummary(message.metadata, t);
  if (message.title) attributes.title = message.title;
  if (message.recordKind) attributes.record_kind = message.recordKind;
  if (toolName) attributes.tool_name = toolName;
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (subagentTaskId) attributes.subagent_task_id = subagentTaskId;
  if (subagentName) attributes.subagent_name = subagentName;
  if (status) attributes.status = status;
  if (summary) attributes.summary = summary;
  return attributes;
}

// parseDirectiveConversationBlocks parses Vulcan triple-colon transcript directives.
// parseDirectiveConversationBlocks 解析 Vulcan 三冒号转录指令。
function parseDirectiveConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const blocks: ConversationBlock[] = [];
  const pattern = /^:::(\S+)([^\n]*)\n([\s\S]*?)\n:::\s*$/gm;
  let lastIndex = 0;
  let blockIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      blockIndex = pushFallbackSegments(blocks, messageId, blockIndex, before.trim());
    }

    const directiveName = match[1] ?? "notice";
    const attributes = parseDirectiveAttributes(match[2] ?? "");
    const body = match[3] ?? "";
    const kind = directiveKinds[directiveName] ?? "notice";
    blocks.push(conversationSpecialBlock(messageId, blockIndex, kind, body, attributes));
    blockIndex += 1;
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    pushFallbackSegments(blocks, messageId, blockIndex, after.trim());
  }

  return blocks;
}

// pushFallbackSegments appends Markdown and diff segments while preserving block indexes.
// pushFallbackSegments 追加 Markdown 与 diff 片段并保持块索引连续。
function pushFallbackSegments(
  blocks: ConversationBlock[],
  messageId: string,
  startIndex: number,
  source: string,
): number {
  let nextIndex = startIndex;
  for (const segment of splitFencedDiffSegments(source)) {
    if (!segment.body.trim()) {
      continue;
    }
    if (segment.kind === "diff") {
      blocks.push(conversationSpecialBlock(messageId, nextIndex, "diff", segment.body, {}));
    } else {
      blocks.push(conversationMarkdownBlock(messageId, nextIndex, segment.body));
    }
    nextIndex += 1;
  }
  return nextIndex;
}

// parseDirectiveAttributes parses loose key=value attributes from one directive header.
// parseDirectiveAttributes 从单条指令头中解析宽松的 key=value 属性。
function parseDirectiveAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const tokens = source.trim().match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];
  for (const token of tokens) {
    const [rawKey, ...rawValueParts] = token.split("=");
    const key = rawKey.trim();
    if (!key) {
      continue;
    }
    const value = rawValueParts.join("=").trim().replace(/^["']|["']$/g, "");
    attributes[key] = value || "true";
  }
  return attributes;
}

// parseFallbackConversationBlocks detects common legacy tool, command, and diff formats.
// parseFallbackConversationBlocks 识别常见旧版工具、命令与 diff 格式。
function parseFallbackConversationBlocks(messageId: string, source: string): ConversationBlock[] {
  const segments = splitFencedDiffSegments(source);
  if (segments.some((segment) => segment.kind === "diff")) {
    return segments.map((segment, index) => {
      if (segment.kind === "diff") {
        return conversationSpecialBlock(messageId, index, "diff", segment.body, {});
      }
      return conversationMarkdownBlock(messageId, index, segment.body);
    });
  }

  const toolKind = detectToolBlockKind(source);
  if (toolKind) {
    return [conversationSpecialBlock(messageId, 0, toolKind, source, detectToolAttributes(source))];
  }

  return [conversationMarkdownBlock(messageId, 0, source)];
}

// splitFencedDiffSegments separates Markdown around diff fences and raw git patches.
// splitFencedDiffSegments 将 diff 围栏与原始 git patch 周围的 Markdown 分离。
function splitFencedDiffSegments(source: string): Array<{ kind: "markdown" | "diff"; body: string }> {
  const segments: Array<{ kind: "markdown" | "diff"; body: string }> = [];
  const pattern = /```(?:diff|patch)\n([\s\S]*?)```|(^diff --git [\s\S]*?)(?=\n\n(?![+\- @\\])|\n#{1,6}\s|\n:::\S+|\s*$)/gm;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(lastIndex, match.index);
    if (before.trim()) {
      segments.push({ kind: "markdown", body: before.trim() });
    }
    segments.push({ kind: "diff", body: (match[1] ?? match[2] ?? "").trim() });
    lastIndex = pattern.lastIndex;
  }

  const after = source.slice(lastIndex);
  if (after.trim()) {
    segments.push({ kind: "markdown", body: after.trim() });
  }

  return segments.length > 0 ? segments : [{ kind: "markdown", body: source }];
}

// detectToolBlockKind classifies legacy text-only tool and runtime blocks.
// detectToolBlockKind 对旧版纯文本工具与运行时块进行分类。
function detectToolBlockKind(source: string): ConversationBlockKind | null {
  if (hasLegacyTurnStatsBlockMarker(source)) {
    return "turn-stats";
  }
  if (hasLegacyToolCallBlockMarker(source)) {
    return "tool-call";
  }
  if (hasLegacyToolResultBlockMarker(source)) {
    return "tool-result";
  }
  if (hasLegacyCommandBlockMarker(source)) {
    return "command";
  }
  if (hasLegacyThinkingBlockMarker(source)) {
    return "thinking";
  }
  return null;
}

// hasLegacyTurnStatsBlockMarker detects explicit legacy turn-statistics transcript blocks.
// hasLegacyTurnStatsBlockMarker 检测明确的旧版轮次统计转录块。
function hasLegacyTurnStatsBlockMarker(source: string): boolean {
  if (/^#{1,6}\s*Turn Stats\b/im.test(source) || /(?:^|\n)\s*Turn Stats\s*(?:[·:：-]|$)/im.test(source)) {
    return true;
  }

  const metricLineCount = source
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .filter(isLegacyTurnStatsMetricLine)
    .length;

  return metricLineCount >= 3;
}

// isLegacyTurnStatsMetricLine returns whether one line is a standalone turn-statistics metric.
// isLegacyTurnStatsMetricLine 返回单行是否为独立轮次统计指标。
function isLegacyTurnStatsMetricLine(line: string): boolean {
  return /^(状态|status)\s*[:：]/iu.test(line)
    || /^(provider\s+(calls?|requests?)|provider\s+(调用|请求))\s*[:：]/iu.test(line)
    || /^(工具调用|tool\s+calls?)\s*[:：]/iu.test(line)
    || /^(输入\s*tokens?|input\s+tokens?)\s*[:：]/iu.test(line)
    || /^(输出\s*tokens?|output\s+tokens?)\s*[:：]/iu.test(line)
    || /^(思考\s*tokens?|reasoning\s+tokens?)\s*[:：]/iu.test(line)
    || /^(总\s*tokens?|total\s+tokens?)\s*[:：]/iu.test(line)
    || /^(耗时|duration)\s*[:：]/iu.test(line)
    || /^(输出速度|output\s+speed)\s*[:：]/iu.test(line)
    || /^(缓存命中\s*tokens?|cached\s+input\s+tokens?)\s*[:：]/iu.test(line);
}

// hasLegacyToolCallBlockMarker detects explicit legacy tool-call transcript blocks.
// hasLegacyToolCallBlockMarker 检测明确的旧版工具调用转录块。
function hasLegacyToolCallBlockMarker(source: string): boolean {
  return /^#{1,6}\s*Tool Call\b/im.test(source)
    || /(?:^|\n)\s*tool_call_id["'`:\s=]+[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*工具调用\s*[·:：]\s*[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*调用\s+[A-Za-z0-9_.:-]+\s+工具\s*(?:进行中|运行中|已完成|失败)?\s*$/im.test(source);
}

// hasLegacyToolResultBlockMarker detects explicit legacy tool-result transcript blocks.
// hasLegacyToolResultBlockMarker 检测明确的旧版工具返回转录块。
function hasLegacyToolResultBlockMarker(source: string): boolean {
  return /^#{1,6}\s*Tool Result\b/im.test(source)
    || /^#{1,6}\s*Result ID\b/im.test(source)
    || /(?:^|\n)\s*tool_result_id["'`:\s=]+[A-Za-z0-9_.:-]+/i.test(source)
    || /(?:^|\n)\s*工具返回\s*[·:：]\s*[A-Za-z0-9_.:-]+/i.test(source);
}

// hasLegacyCommandBlockMarker detects explicit command-session transcript blocks without matching ordinary tool lists.
// hasLegacyCommandBlockMarker 检测明确的命令会话转录块，避免误匹配普通工具清单。
function hasLegacyCommandBlockMarker(source: string): boolean {
  const normalized = source.toLowerCase();
  return normalized.includes("command_session_id")
    || /^#{1,6}\s*(command session|命令会话)\b/im.test(source)
    || /^(command session|命令会话)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:command_session|command session|命令会话)/i.test(source);
}

// hasLegacyThinkingBlockMarker detects explicit thinking transcript blocks without matching normal prose.
// hasLegacyThinkingBlockMarker 检测明确的思考转录块，避免误匹配普通正文。
function hasLegacyThinkingBlockMarker(source: string): boolean {
  return /^#{1,6}\s*(thinking|思考过程|思考)\b/im.test(source)
    || /^(thinking|思考过程|思考)\s*[:：]/im.test(source)
    || /(?:类型|kind|type)\s*[:：=]\s*(?:thinking|思考过程|思考)/i.test(source);
}

// detectToolAttributes extracts only explicit legacy identifiers from tool text.
// detectToolAttributes 仅从旧版工具文本中提取显式标识。
function detectToolAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const toolCallId = source.match(/tool_call_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const toolResultId = extractToolResultIdFromText(source);
  const commandSessionId = source.match(/command_session_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i)?.[1];
  const hasToolMarker = hasLegacyToolCallBlockMarker(source) || hasLegacyToolResultBlockMarker(source);
  if (toolCallId) attributes.tool_call_id = toolCallId;
  if (toolResultId) attributes.tool_result_id = toolResultId;
  if (commandSessionId) attributes.command_session_id = commandSessionId;
  if (hasToolMarker) attributes.tool_evidence = "true";
  return attributes;
}

// conversationMarkdownBlock creates a standard Markdown block descriptor.
// conversationMarkdownBlock 创建标准 Markdown 块描述。
function conversationMarkdownBlock(messageId: string, index: number, body: string): ConversationBlock {
  return {
    id: `${messageId}-markdown-${index}`,
    kind: "markdown",
    title: "Markdown",
    body,
    summary: compactTextPreview(body),
  };
}

// conversationSpecialBlock creates a semantic block with compact summary metadata.
// conversationSpecialBlock 创建带紧凑摘要元数据的语义块。
function conversationSpecialBlock(
  messageId: string,
  index: number,
  kind: ConversationBlockKind,
  body: string,
  attributes: Record<string, string>,
  rawMetadata?: unknown,
): ConversationBlock {
  const toolName = attributes.tool_name;
  const title = attributes.title ?? blockTitle(kind, toolName);
  const mediaAttachments = generatedMediaAttachmentsFromMetadata(rawMetadata);
  const toolEvidence = conversationBlockHasToolEvidence(kind, attributes, rawMetadata);
  const planArtifact = kind === "plan-artifact" ? {
    name: attributes.plan_name ?? title,
    path: attributes.plan_path ?? "",
    content: attributes.plan_content ?? body.trim(),
    updatedAt: attributes.plan_updated_at ?? "",
  } : undefined;
  const compactionSectionId = metadataString(rawMetadata, ["context_compaction_section_id"]);
  const compactionRole = metadataString(rawMetadata, ["context_compaction_role"]);
  const compactionState = metadataString(rawMetadata, ["context_compaction_state"]);
  const compactionTrigger = metadataString(rawMetadata, ["context_compaction_trigger"]);
  const compactionBudgetNote = metadataString(rawMetadata, ["context_compaction_budget_note"]);
  const status = mediaAttachments.length > 0
    ? "completed"
    : normalizeToolDisplayState(attributes.status);
  return {
    id: `${messageId}-${kind}-${index}`,
    kind,
    title,
    body: body.trim(),
    summary: attributes.summary ?? compactTextPreview(body),
    status,
    toolName,
    toolEvidence,
    toolCallId: attributes.tool_call_id,
    toolResultId: attributes.tool_result_id,
    commandSessionId: attributes.command_session_id,
    subagentTaskId: attributes.subagent_task_id,
    subagentName: attributes.subagent_name,
    rawMetadata,
    mediaAttachments: mediaAttachments.length > 0 ? mediaAttachments : undefined,
    compactionSectionId: compactionSectionId ?? undefined,
    compactionRole: compactionRole ?? undefined,
    compactionState: compactionState ?? undefined,
    compactionTrigger: compactionTrigger ?? undefined,
    compactionBudgetNote: compactionBudgetNote ?? undefined,
    planArtifact,
  };
}

// planArtifactAttributesFromMetadata extracts a submitted Plan from host_result metadata.
// planArtifactAttributesFromMetadata 从 host_result 元数据中提取已提交计划。
function planArtifactAttributesFromMetadata(metadata: unknown): { name: string; path: string; content: string; updatedAt: string } | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "plan_artifact") {
    const payload = metadataValue(metadata, ["host_result.payload", "hostResult.payload", "payload"]);
    return planArtifactAttributesFromPayload(payload);
  }
  return null;
}

// planArtifactAttributesFromPayload extracts one submitted Plan from a host payload-like object.
// planArtifactAttributesFromPayload 从类宿主载荷对象中提取单个已提交计划。
function planArtifactAttributesFromPayload(payload: unknown): { name: string; path: string; content: string; updatedAt: string } | null {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return null;
  }
  const object = payload as Record<string, unknown>;
  const name = stringFromUnknown(object.name) ?? "Plan";
  const path = stringFromUnknown(object.path) ?? "";
  const content = stringFromUnknown(object.content) ?? "";
  const updatedAt = stringFromUnknown(object.updatedAt ?? object.updated_at) ?? "";
  if (!content.trim()) {
    return null;
  }
  return { name, path, content, updatedAt };
}

// stringFromUnknown returns one non-empty string from a JSON-ish value.
// stringFromUnknown 从类 JSON 值中返回一条非空字符串。
function stringFromUnknown(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

// compactTextPreview returns one short single-line preview for cards and modal headers.
// compactTextPreview 返回用于卡片与弹窗标题区的单行短预览。
function compactTextPreview(source: string): string {
  const text = source
    .replace(/```[\s\S]*?```/g, "code block")
    .replace(/[#>*_`|~-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!text) {
    return "No body content";
  }
  return text.length > 140 ? `${text.slice(0, 140)}...` : text;
}
