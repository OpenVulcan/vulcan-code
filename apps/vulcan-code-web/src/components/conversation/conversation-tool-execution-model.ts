import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type {
  ToolExecutionConversationBlock,
  ToolExecutionConversationBlockKind,
} from "./conversation-tool-execution-types";

// buildToolExecutionDetailBody builds the expensive modal body only when the user opens details.
// buildToolExecutionDetailBody 仅在用户打开详情时构建成本较高的弹窗正文。
export function buildToolExecutionDetailBody(blocks: ToolExecutionConversationBlock[]): string {
  return blocks
    .map((block) => {
      const sections = [`## ${toolBlockTitle(block.kind, block.toolName)}`];
      const argumentsValue = metadataValue(block.rawMetadata, ["arguments", "payload.arguments"]);
      if (argumentsValue !== undefined) {
        sections.push(`### Arguments\n\n\`\`\`json\n${JSON.stringify(argumentsValue, null, 2)}\n\`\`\``);
      }
      const resultStatus = metadataString(block.rawMetadata, ["result.status"]);
      const detailRecordId = metadataString(block.rawMetadata, [
        "result.detail_ref.record_id",
        "result.detailRef.recordId",
      ]);
      if (resultStatus || detailRecordId) {
        sections.push(`### Result\n\n${[
          resultStatus ? `Status: ${resultStatus}` : null,
          detailRecordId ? `Detail Record: ${detailRecordId}` : null,
        ].filter(Boolean).join("\n")}`);
      }
      if (block.body.trim()) {
        sections.push(block.body);
      }
      return sections.join("\n\n");
    })
    .join("\n\n---\n\n");
}

// toolExecutionSummary builds the compact transcript row summary for one tool execution.
// toolExecutionSummary 为单次工具执行构建紧凑转录行摘要。
export function toolExecutionSummary(blocks: ToolExecutionConversationBlock[], sessionRunning: boolean): string {
  const commandBlock = blocks.find((block) => block.kind === "command");
  const toolName = toolExecutionName(blocks);
  const displayState = toolExecutionDisplayState(blocks, sessionRunning);
  const statusLabel = toolDisplayStateLabel(displayState);
  const identifier = toolExecutionIdentifier(blocks);
  const errorPreview = displayState === "failed" ? toolExecutionErrorPreview(blocks) : null;
  if (commandBlock && !toolName) {
    return commandBlock.summary.includes("命令")
      ? commandBlock.summary
      : "已运行 1 条命令";
  }

  if (toolName) {
    return `调用 ${toolName} 工具 · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }
  return `工具执行 · ${statusLabel}${identifier ? ` · ${identifier}` : ""}${errorPreview ? ` · ${errorPreview}` : ""}`;
}

// toolExecutionName resolves the best human-visible tool name across merged call and result blocks.
// toolExecutionName 从合并后的调用与返回块中解析最佳人类可见工具名。
export function toolExecutionName(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const name = block.toolName ?? extractToolNameFromText(block.title);
    if (name) {
      return name;
    }
  }
  return null;
}

// toolExecutionIdentifier resolves the most useful fallback identifier for a tool row.
// toolExecutionIdentifier 为工具行解析最有用的兜底标识。
export function toolExecutionIdentifier(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const identifier = block.toolCallId
      ?? block.toolResultId
      ?? block.commandSessionId
      ?? block.taskSessionId
      ?? metadataString(block.rawMetadata, [
        "tool_call_id",
        "toolCallId",
        "payload.tool_call_id",
        "payload.toolCallId",
        "host_result.payload.tool_call_id",
        "hostResult.payload.toolCallId",
        "task_session_id",
        "taskSessionId",
        "payload.task_session_id",
        "payload.taskSessionId",
        "host_result.payload.task_session_id",
        "hostResult.payload.taskSessionId",
      ]);
    if (identifier) {
      return shortenToolExecutionIdentifier(identifier);
    }
  }
  return null;
}

// toolExecutionDetailRef resolves the hidden full-detail reference for one tool execution.
// toolExecutionDetailRef 解析单次工具执行对应的隐藏完整详情引用。
export function toolExecutionDetailRef(blocks: ToolExecutionConversationBlock[]): Omit<ConversationDetailRef, "sessionId"> | null {
  for (const block of blocks) {
    const recordId = metadataString(block.rawMetadata, [
      "result.detail_ref.record_id",
      "result.detailRef.recordId",
    ]);
    if (recordId) {
      const seq = numericMetadata(metadataValue(block.rawMetadata, [
        "result.detail_ref.seq",
        "result.detailRef.seq",
      ]));
      const messageIndex = numericMetadata(metadataValue(block.rawMetadata, [
        "result.detail_ref.message_index",
        "result.detailRef.messageIndex",
      ]));
      return {
        recordId,
        ...(seq !== null ? { seq: Math.trunc(seq) } : {}),
        ...(messageIndex !== null ? { messageIndex: Math.trunc(messageIndex) } : {}),
      };
    }
  }
  return null;
}

// shortenToolExecutionIdentifier keeps long UUID-like ids readable in compact rows.
// shortenToolExecutionIdentifier 让较长的 UUID 类标识在紧凑行中保持可读。
function shortenToolExecutionIdentifier(identifier: string): string {
  const trimmed = identifier.trim();
  if (trimmed.length <= 18) {
    return trimmed;
  }
  return `${trimmed.slice(0, 8)}...${trimmed.slice(-6)}`;
}

// toolExecutionErrorPreview extracts a compact failure reason from metadata or body text.
// toolExecutionErrorPreview 从元数据或正文中提取紧凑失败原因。
export function toolExecutionErrorPreview(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const metadataError = metadataString(block.rawMetadata, [
      "result.error",
      "result.reason",
      "host_result.payload.message",
      "hostResult.payload.message",
      "host_result.payload.error",
      "hostResult.payload.error",
      "host_result.payload.reason",
      "hostResult.payload.reason",
      "payload.message",
      "payload.error",
      "payload.reason",
      "message",
      "error",
      "reason",
    ]);
    if (metadataError) {
      return compactToolExecutionErrorPreview(metadataError);
    }
  }
  const failedBlock = blocks.find((block) => normalizeToolDisplayState(block.status) === "failed")
    ?? blocks.find((block) => /失败|报错|错误|failed|error|panic/i.test(block.body));
  const preview = failedBlock?.body || failedBlock?.summary;
  return preview ? compactToolExecutionErrorPreview(preview) : null;
}

// preferredTaskSessionDisplayBlock keeps routed task-session rows anchored to the newest terminal metadata.
// preferredTaskSessionDisplayBlock 让已路由 task-session 行优先使用最新终态元数据作为展示锚点。
export function preferredTaskSessionDisplayBlock(blocks: ToolExecutionConversationBlock[]): ToolExecutionConversationBlock | null {
  const taskBlocks = blocks.filter((block) => block.taskSessionId);
  if (!taskBlocks.length) {
    return null;
  }
  return taskBlocks.find((block) => {
    const state = normalizeToolDisplayState(block.status);
    return state === "failed" || state === "stopped" || state === "completed";
  }) ?? taskBlocks.find((block) => block.kind === "tool-call") ?? taskBlocks[0] ?? null;
}

// toolExecutionDisplayState resolves the stable compact display state for one tool execution row.
// toolExecutionDisplayState 解析单条工具执行行使用的稳定紧凑显示状态。
export function toolExecutionDisplayState(blocks: ToolExecutionConversationBlock[], sessionRunning: boolean): "failed" | "completed" | "running" | "stopped" {
  const taskSessionState = taskSessionExecutionDisplayState(blocks);
  if (taskSessionState) {
    return taskSessionState;
  }
  const explicitStates = blocks
    .map((block) => normalizeToolDisplayState(block.status))
    .filter((state): state is string => Boolean(state));
  if (explicitStates.includes("failed")) {
    return "failed";
  }
  if (explicitStates.includes("stopped")) {
    return "stopped";
  }
  if (explicitStates.includes("completed")) {
    return "completed";
  }
  if (blocks.some((block) => block.toolResultId || toolExecutionDetailRef([block]))) {
    return "completed";
  }
  if (blocks.some((block) => block.kind === "tool-result" || block.kind === "command")) {
    return "completed";
  }
  if (!sessionRunning) {
    return "stopped";
  }
  return "running";
}

// toolExecutionMediaAttachments resolves generated media outputs across merged tool call/result rows.
// toolExecutionMediaAttachments 从合并后的工具调用/返回行中解析生成媒体输出。
export function toolExecutionMediaAttachments(blocks: ToolExecutionConversationBlock[]): GeneratedMediaAttachment[] {
  const seen = new Set<string>();
  const attachments: GeneratedMediaAttachment[] = [];
  for (const block of blocks) {
    for (const attachment of block.mediaAttachments ?? []) {
      const key = `${attachment.kind}:${attachment.localPath}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      attachments.push(attachment);
    }
  }
  return attachments;
}

// toolDisplayStateLabel returns the localized label used inside compact tool rows.
// toolDisplayStateLabel 返回紧凑工具行中使用的本地化状态文案。
export function toolDisplayStateLabel(status: "failed" | "completed" | "running" | string): string {
  if (status === "failed") return "失败";
  if (status === "completed") return "已完成";
  if (status === "stopped") return "已停止";
  return "进行中";
}

// conversationDetailRefCacheKey builds a stable cache key for one hidden tool detail reference.
// conversationDetailRefCacheKey 为单条隐藏工具详情引用构建稳定缓存键。
export function conversationDetailRefCacheKey(detailRef: ConversationDetailRef): string {
  return [
    detailRef.sessionId,
    detailRef.recordId ?? "",
    detailRef.toolCallId ?? "",
    detailRef.seq ?? "",
    detailRef.messageIndex ?? "",
  ].join("\u001f");
}

// compactToolExecutionErrorPreview normalizes verbose error text for one-line tool summaries.
// compactToolExecutionErrorPreview 将冗长错误文本规整为工具摘要中的单行内容。
function compactToolExecutionErrorPreview(source: string): string | null {
  const line = source
    .split(/\r?\n/u)
    .map((part) => part.trim())
    .find((part) => part.length > 0 && !/^#+\s/.test(part));
  if (!line) {
    return null;
  }
  return compactTextPreview(line).slice(0, 96);
}

// taskSessionExecutionDisplayState resolves the visible state for routed task-session rows from explicit task metadata.
// taskSessionExecutionDisplayState 从显式 task 元数据解析已路由 task-session 行的可见状态。
function taskSessionExecutionDisplayState(blocks: ToolExecutionConversationBlock[]): "failed" | "completed" | "running" | "stopped" | null {
  const taskBlocks = blocks.filter((block) => block.taskSessionId);
  if (!taskBlocks.length) {
    return null;
  }
  const states = taskBlocks
    .map((block) => normalizeToolDisplayState(block.status))
    .filter((state): state is string => Boolean(state));
  if (states.includes("failed")) {
    return "failed";
  }
  if (states.includes("stopped")) {
    return "stopped";
  }
  if (states.includes("completed")) {
    return "completed";
  }
  if (states.includes("running")) {
    return "running";
  }
  return null;
}

// toolBlockTitle returns a localized title for one semantic transcript block.
// toolBlockTitle 返回单个语义转录块的本地化标题。
function toolBlockTitle(kind: ToolExecutionConversationBlockKind, toolName?: string): string {
  if (kind === "tool-call") return toolName ? `工具调用 · ${toolName}` : "工具调用";
  if (kind === "tool-result") return toolName ? `工具返回 · ${toolName}` : "工具返回";
  if (kind === "command") return "命令会话";
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return "思考过程";
  if (kind === "turn-stats") return "Turn Stats";
  if (kind === "notice") return "运行提示";
  return "Markdown";
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

// numericMetadata reads a number from unknown JSON values.
// numericMetadata 从未知 JSON 值中读取数字。
function numericMetadata(value: unknown): number | null {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

// normalizeToolDisplayState maps provider and host state words into WebUI display states.
// normalizeToolDisplayState 将 Provider 与宿主状态词映射为 WebUI 展示状态。
function normalizeToolDisplayState(status?: string | null): string | undefined {
  if (!status?.trim()) {
    return undefined;
  }
  const normalized = status.trim().toLowerCase();
  if (normalized === "failed" || normalized === "error" || normalized === "errored" || normalized === "failure") {
    return "failed";
  }
  if (normalized === "completed" || normalized === "complete" || normalized === "success" || normalized === "succeeded" || normalized === "ready") {
    return "completed";
  }
  if (normalized === "stopped" || normalized === "cancelled" || normalized === "canceled" || normalized === "aborted") {
    return "stopped";
  }
  if (normalized === "running" || normalized === "pending" || normalized === "preview" || normalized === "in_progress") {
    return "running";
  }
  if (status.includes("失败") || status.includes("错误")) {
    return "failed";
  }
  if (status.includes("完成") || status.includes("成功")) {
    return "completed";
  }
  if (status.includes("运行") || status.includes("进行")) {
    return "running";
  }
  return status.trim();
}

// extractToolNameFromText recovers a tool name from legacy localized titles and Markdown bodies.
// extractToolNameFromText 从旧版本地化标题与 Markdown 正文中恢复工具名称。
function extractToolNameFromText(source: string): string | null {
  const normalized = source.replace(/\r\n/g, "\n");
  const patterns = [
    /##\s*Tool Call\s*\n+\s*`([^`]+)`/im,
    /##\s*Tool Result\s*\n+\s*`([^`]+)`/im,
    /tool_name["'`:\s=]+([A-Za-z0-9_.:-]+)/i,
    /"tool_name"\s*:\s*"([^"]+)"/i,
    /"toolName"\s*:\s*"([^"]+)"/i,
    /工具调用\s*[·:：]\s*[A-Za-z0-9_.:-]+\s*[·:：]\s*([A-Za-z0-9_.:-]+)/,
    /(?:工具调用|工具返回)\s*[·:：]?\s*([A-Za-z0-9_.:-]+)/,
    /调用\s+([A-Za-z0-9_.:-]+)\s+工具/,
    /Tool (?:Call|Result)\s+([A-Za-z0-9_.:-]+)/i,
    /Call\s+([A-Za-z0-9_.:-]+)/i,
  ];
  for (const pattern of patterns) {
    const value = normalized.match(pattern)?.[1]?.trim();
    if (value && !genericToolNameLabels.has(value)) {
      return value;
    }
  }
  return null;
}

// compactTextPreview returns one short single-line preview for cards and modal headers.
// compactTextPreview 返回用于卡片与弹窗标题区的单行短预览。
function compactTextPreview(source: string): string {
  const text = source
    .replace(/```[\s\S]*?```/g, "代码块")
    .replace(/[#>*_`|~-]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!text) {
    return "无正文内容";
  }
  return text.length > 140 ? `${text.slice(0, 140)}...` : text;
}

// genericToolNameLabels lists localized labels that are headings rather than actual tool names.
// genericToolNameLabels 列出只是标题而不是实际工具名的本地化标签。
const genericToolNameLabels = new Set(["工具调用", "工具返回", "Tool Call", "Tool Result", "Raw Tool Result"]);
