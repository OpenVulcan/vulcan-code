import type { ConversationMessage } from "@/types";
import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
} from "@/components/conversation/conversation-metadata";
import type { ConversationBlockKind } from "@/components/conversation/conversation-model";
import type { TranslationKey } from "@/i18n";

// taskSessionIdFromToolMetadata extracts the stable child task-session id from host task metadata.
// taskSessionIdFromToolMetadata 从宿主 task 元数据中提取稳定的子任务会话标识。
export function taskSessionIdFromToolMetadata(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "task_session") {
    return metadataString(metadata, [
      "host_result.payload.task_session_id",
      "hostResult.payload.taskSessionId",
      "payload.task_session_id",
      "payload.taskSessionId",
      "task_session_id",
    ]);
  }
  return metadataString(metadata, [
    "task_session_id",
  ]);
}

// metadataToolFailureStatus detects host-side tool failures that are stored as structured metadata.
// metadataToolFailureStatus 识别以结构化元数据保存的宿主侧工具失败状态。
export function metadataToolFailureStatus(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (hostKind === "tool_error") {
    return "failed";
  }
  return null;
}

// turnStatsStatus reads the compact turn-stats terminal status.
// turnStatsStatus 读取紧凑轮次统计终态。
export function turnStatsStatus(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const status = stats?.status;
  return typeof status === "string" && status.trim() ? status.trim() : null;
}

// turnStatsSummary builds the compact summary line used by WebUI tool metadata.
// turnStatsSummary 构建 WebUI 工具元数据使用的紧凑摘要行。
export function turnStatsSummary(
  // Metadata object that may carry turn statistics.
  // 可能携带轮次统计的元数据对象。
  metadata: unknown,
  // Optional translator used by React surfaces that must follow language switching.
  // React 界面用于跟随语言切换的可选翻译函数。
  t?: (key: TranslationKey) => string,
): string | null {
  const stats = turnStatsObject(metadata);
  if (!stats) {
    return null;
  }
  const translate = t ?? defaultTurnStatsTranslation;
  const parts = [
    formatTurnStatsTranslation(translate, "conversation.stats.providerCalls", {
      count: String(numberMetadata(stats.provider_call_count ?? stats.providerCallCount)),
    }),
    formatTurnStatsTranslation(translate, "conversation.stats.toolCalls", {
      count: String(numberMetadata(stats.tool_call_count ?? stats.toolCallCount)),
    }),
  ];
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  if (totalTokens > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.totalTokens", {
      count: String(totalTokens),
    }));
  }
  const cachedTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  if (cachedTokens > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.cachedTokens", {
      count: String(cachedTokens),
    }));
  }
  const speed = numericMetadata(stats.output_tokens_per_second ?? stats.outputTokensPerSecond);
  if (speed && Number.isFinite(speed) && speed > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.outputSpeed", {
      speed: speed.toFixed(1),
    }));
  }
  return parts.join(" · ");
}

// isSubagentToolName returns whether one tool name belongs to the built-in subagent tool family.
// isSubagentToolName 判断单个工具名是否属于内置子代理工具族。
export function isSubagentToolName(toolName?: string | null): boolean {
  const normalized = toolName?.trim().toLowerCase();
  return normalized === "spawn_agent"
    || normalized === "send_message"
    || normalized === "followup_task"
    || normalized === "wait_agent"
    || normalized === "list_agents"
    || normalized === "close_agent";
}

// subagentToolDisplayName extracts the visible child-agent name from tool metadata.
// subagentToolDisplayName 从工具元数据中提取可见的子代理名称。
export function subagentToolDisplayName(metadata: unknown, toolName?: string | null): string | null {
  const metadataToolName = toolName ?? metadataString(metadata, [
    "tool_name",
    "host_result.tool_name",
    "hostResult.toolName",
  ]);
  const taskSessionId = taskSessionIdFromToolMetadata(metadata);
  if (!isSubagentToolName(metadataToolName) && !taskSessionId) {
    return null;
  }
  return metadataString(metadata, [
    "host_result.payload.display_title",
    "hostResult.payload.displayTitle",
    "payload.display_title",
    "payload.displayTitle",
    "display_title",
    "displayTitle",
    "arguments.task_name",
    "arguments.task_id",
    "arguments.agent_id",
    "arguments.child_thread_id",
    "arguments.target",
    "arguments.name",
    "host_result.payload.task_name",
    "hostResult.payload.taskName",
    "payload.task_name",
    "payload.taskName",
    "host_result.payload.description",
    "hostResult.payload.description",
    "payload.description",
    "description",
    "host_result.payload.subagent",
    "hostResult.payload.subagent",
    "payload.subagent",
    "arguments.agent_type",
    "host_result.payload.task_id",
    "hostResult.payload.taskId",
    "payload.task_id",
    "payload.taskId",
    "task_id",
    "taskId",
  ]);
}

// defaultTurnStatsTranslation keeps non-React callers on the historical English fallback.
// defaultTurnStatsTranslation 让非 React 调用方保持历史英文回退。
function defaultTurnStatsTranslation(key: TranslationKey): string {
  switch (key) {
    case "conversation.stats.providerCalls":
      return "{count} calls";
    case "conversation.stats.toolCalls":
      return "{count} tools";
    case "conversation.stats.totalTokens":
      return "{count} tokens";
    case "conversation.stats.cachedTokens":
      return "Cache hit {count}";
    case "conversation.stats.outputSpeed":
      return "{speed}/s";
    default:
      return key;
  }
}

// formatTurnStatsTranslation replaces simple named placeholders in compact stats labels.
// formatTurnStatsTranslation 替换紧凑统计标签中的简单具名占位符。
function formatTurnStatsTranslation(
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
  // Translation key to format.
  // 需要格式化的翻译键。
  key: TranslationKey,
  // Placeholder values.
  // 占位符取值。
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
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
  if (normalized === "completed" || normalized === "complete" || normalized === "success" || normalized === "succeeded") {
    return "completed";
  }
  if (normalized === "stopped" || normalized === "cancelled" || normalized === "canceled" || normalized === "aborted") {
    return "stopped";
  }
  if (normalized === "running" || normalized === "pending" || normalized === "in_progress") {
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
  if (attributes.tool_evidence === "true" || attributes.tool_call_id || attributes.tool_result_id || attributes.command_session_id || attributes.task_session_id) {
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
  return false;
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
  const hostKind = metadataString(rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && ["tool_error", "tool_result", "tool_call", "task_session", "command_session", "command_run", "change_set"].includes(hostKind)) {
    return true;
  }
  return false;
}

// toolMetadataContainsReturnPayload returns whether a tool result carries an API-level returned payload.
// toolMetadataContainsReturnPayload 返回工具结果是否携带 API 层已返回载荷。
export function toolMetadataContainsReturnPayload(metadata: unknown, body?: string): boolean {
  if (typeof body === "string" && body.trim()) {
    return true;
  }
  return Boolean(
    metadataString(metadata, ["tool_result_id"])
      ?? metadataValue(metadata, ["host_result", "hostResult"]),
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
