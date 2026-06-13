import type { ConversationMessage } from "@/types";
import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
} from "@/components/conversation/conversation-metadata";
import type { ConversationBlockKind } from "@/components/conversation/conversation-model";

// genericToolNameLabels lists localized labels that are headings rather than actual tool names.
// genericToolNameLabels 列出只是标题而不是实际工具名的本地化标签。
const genericToolNameLabels = new Set(["工具调用", "工具返回", "Tool Call", "Tool Result", "Raw Tool Result"]);

// taskSessionIdFromToolMetadata extracts the routed child task-session id from host task metadata.
// taskSessionIdFromToolMetadata 从宿主 task 元数据中提取路由用的子任务会话标识。
export function taskSessionIdFromToolMetadata(metadata: unknown): string | null {
  return metadataString(metadata, [
    "payload.task_session_id",
    "payload.taskSessionId",
    "host_result.payload.task_session_id",
    "hostResult.payload.taskSessionId",
    "task_session_id",
    "taskSessionId",
  ]);
}

// metadataToolFailureStatus detects host-side tool failures that are stored as structured metadata.
// metadataToolFailureStatus 识别以结构化元数据保存的宿主侧工具失败状态。
export function metadataToolFailureStatus(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "tool_error") {
    return "failed";
  }
  const payloadStatus = metadataString(metadata, ["payload.status", "status"]);
  return payloadStatus === "failed" || payloadStatus === "error" ? "failed" : null;
}

// turnStatsStatus reads the compact turn-stats terminal status.
// turnStatsStatus 读取紧凑轮次统计终态。
export function turnStatsStatus(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const status = stats?.status;
  return typeof status === "string" && status.trim() ? status.trim() : null;
}

// turnStatsSummary builds the same compact summary line used by the legacy GUI.
// turnStatsSummary 构建与旧 GUI 一致的紧凑摘要行。
export function turnStatsSummary(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  if (!stats) {
    return null;
  }
  const parts = [
    `${numberMetadata(stats.provider_call_count ?? stats.providerCallCount)} 次调用`,
    `${numberMetadata(stats.tool_call_count ?? stats.toolCallCount)} 个工具`,
  ];
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  if (totalTokens > 0) {
    parts.push(`${totalTokens} tokens`);
  }
  const cachedTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  if (cachedTokens > 0) {
    parts.push(`缓存命中 ${cachedTokens}`);
  }
  const speed = numericMetadata(stats.output_tokens_per_second ?? stats.outputTokensPerSecond);
  if (speed && Number.isFinite(speed) && speed > 0) {
    parts.push(`${speed.toFixed(1)}/s`);
  }
  return parts.join(" · ");
}

// extractToolResultIdFromText recovers host result ids from legacy Markdown or compact plain text.
// extractToolResultIdFromText 从旧版 Markdown 或紧凑纯文本中恢复宿主结果标识。
export function extractToolResultIdFromText(source: string): string | null {
  const normalized = source.replace(/\r\n/g, "\n");
  const patterns = [
    /tool_result_id["'`:\s=]+([A-Za-z0-9_.:-]+)/i,
    /##\s*Result ID\s*\n+\s*`([^`]+)`/im,
    /(?:^|\n)\s*Result ID\s*\n+\s*`?([A-Za-z0-9_.:-]+)`?/im,
    /(?:^|\n)\s*Result ID\s+([A-Za-z0-9_.:-]+)/im,
  ];
  for (const pattern of patterns) {
    const value = normalized.match(pattern)?.[1]?.trim();
    if (value) {
      return value;
    }
  }
  return null;
}

// extractToolNameFromText recovers a tool name from legacy localized titles and Markdown bodies.
// extractToolNameFromText 从旧版本地化标题与 Markdown 正文中恢复工具名称。
export function extractToolNameFromText(source: string): string | null {
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

// extractToolStatusFromText recovers a display state from legacy tool output text.
// extractToolStatusFromText 从旧版工具输出文本中恢复显示状态。
export function extractToolStatusFromText(source: string): string | null {
  const normalized = source.toLowerCase();
  if (/失败|报错|错误|failed|error|panic|exit code\s*-\d|退出码\s*-\d/i.test(source)) {
    return "failed";
  }
  if (/已完成|完成|completed|success|succeeded/i.test(source)) {
    return "completed";
  }
  if (/运行中|进行中|running|pending|in progress/i.test(normalized)) {
    return "running";
  }
  return null;
}

// normalizeToolDisplayState maps provider and host state words into WebUI display states.
// normalizeToolDisplayState 将 Provider 与宿主状态词映射为 WebUI 展示状态。
export function normalizeToolDisplayState(status?: string | null): string | undefined {
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

// structuredKindLooksLikeToolBlock returns whether a structured kind needs tool evidence before special rendering.
// structuredKindLooksLikeToolBlock 返回结构化类型是否需要工具证据才能按特殊块渲染。
export function structuredKindLooksLikeToolBlock(kind: ConversationBlockKind): boolean {
  return kind === "tool-call" || kind === "tool-result" || kind === "command" || kind === "diff";
}

// structuredMessageHasToolEvidence prevents assistant prose from being misrendered as a failed tool row.
// structuredMessageHasToolEvidence 防止助手正文被误渲染为失败工具行。
export function structuredMessageHasToolEvidence(message: ConversationMessage, attributes: Record<string, string>): boolean {
  if (attributes.tool_evidence === "true" || attributes.tool_name || attributes.tool_call_id || attributes.tool_result_id || attributes.command_session_id || attributes.task_session_id) {
    return true;
  }
  const hostKind = metadataString(message.metadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind)) {
    return true;
  }
  const payloadTool = metadataString(message.metadata, [
    "payload.tool",
    "host_result.payload.tool",
    "hostResult.payload.tool",
    "payload.tool_name",
    "host_result.payload.tool_name",
    "hostResult.payload.toolName",
  ]);
  if (payloadTool) {
    return true;
  }
  const title = message.title ?? "";
  return /调用\s+\S+\s+工具|工具调用|工具返回|Tool Call|Tool Result|Command Session|命令会话/i.test(title);
}

// conversationBlockHasToolEvidence separates real tool metadata from prose-derived display text.
// conversationBlockHasToolEvidence 将真实工具元数据与正文推断出的展示文本分离。
export function conversationBlockHasToolEvidence(
  kind: ConversationBlockKind,
  attributes: Record<string, string>,
  rawMetadata?: unknown,
): boolean {
  if (!structuredKindLooksLikeToolBlock(kind)) {
    return false;
  }
  if (attributes.tool_evidence === "true" || attributes.tool_call_id || attributes.tool_result_id || attributes.command_session_id || attributes.task_session_id) {
    return true;
  }
  if (attributes.tool_name || attributes.name) {
    return true;
  }
  const hostKind = metadataString(rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind)) {
    return true;
  }
  return Boolean(metadataString(rawMetadata, [
    "payload.tool",
    "host_result.payload.tool",
    "hostResult.payload.tool",
    "payload.tool_name",
    "host_result.payload.tool_name",
    "hostResult.payload.toolName",
  ]));
}

// toolMetadataContainsReturnPayload returns whether a tool result carries an API-level returned payload.
// toolMetadataContainsReturnPayload 返回工具结果是否携带 API 层已返回载荷。
export function toolMetadataContainsReturnPayload(metadata: unknown, body?: string): boolean {
  if (typeof body === "string" && body.trim()) {
    return true;
  }
  return Boolean(
    metadataString(metadata, ["tool_result_id", "toolResultId", "payload.tool_result_id", "payload.toolResultId"])
      ?? metadataValue(metadata, ["host_result", "hostResult", "payload"]),
  );
}

// turnStatsObject reads the structured turn-stats payload from transcript metadata.
// turnStatsObject 从转录元数据中读取结构化轮次统计载荷。
function turnStatsObject(metadata: unknown): Record<string, unknown> | null {
  if (!metadata || typeof metadata !== "object") {
    return null;
  }
  const value = (metadata as Record<string, unknown>).turn_stats ?? (metadata as Record<string, unknown>).turnStats;
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}
