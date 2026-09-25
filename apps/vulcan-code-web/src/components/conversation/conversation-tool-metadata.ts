import type { ConversationMessage } from "@/types";
import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
} from "@/components/conversation/conversation-metadata";
import type { ConversationBlockKind } from "@/components/conversation/conversation-model";
import type { TranslationKey } from "@/i18n";
import { formatTokenCount, formatTokenSpeedValue } from "@/lib/usage-format";

// SUBAGENT_HOST_KIND_VALUES lists every host kind emitted by built-in subagent orchestration tools.
// SUBAGENT_HOST_KIND_VALUES 列出内置子智能体编排工具会产生的全部宿主类型。
const SUBAGENT_HOST_KIND_VALUES = [
  "subagent_task",
  "subagent_message",
  "subagent_control",
  "subagent_status",
  "subagent_wait",
  "subagent_feedback",
] as const;

// SUBAGENT_HOST_KINDS provides constant-time lookup for subagent host kinds.
// SUBAGENT_HOST_KINDS 为子智能体宿主类型提供常量时间查询。
const SUBAGENT_HOST_KINDS = new Set<string>(SUBAGENT_HOST_KIND_VALUES);

// COLLABORATION_TOOL_NAMES lists the exact native agent-collaboration tool ids.
// COLLABORATION_TOOL_NAMES 列出原生代理协作工具的精确标识。
const COLLABORATION_TOOL_NAMES = new Set([
  "ask_question",
  "ask_parent",
  "reply_question",
  "wait_question",
  "wait_agents_result",
  "cancel_question",
  "send_notification",
  "list_sessions",
  "read_session",
  "send_session_message",
  "wait_session",
]);

// SUBAGENT_TOOL_HOST_KINDS lists host kinds that should render as tool evidence.
// SUBAGENT_TOOL_HOST_KINDS 列出应渲染为工具证据的宿主类型。
const TOOL_EVIDENCE_HOST_KINDS = new Set([
  "tool_error",
  "tool_result",
  "tool_call",
  ...SUBAGENT_HOST_KIND_VALUES,
  "command_session",
  "command_run",
  "change_set",
]);

// isSubagentHostKind returns whether one host result kind belongs to subagent orchestration.
// isSubagentHostKind 判断单个宿主结果类型是否属于子智能体编排。
export function isSubagentHostKind(hostKind?: string | null): boolean {
  const normalized = hostKind?.trim().toLowerCase();
  return Boolean(normalized && SUBAGENT_HOST_KINDS.has(normalized));
}

// isCollaborationToolName returns whether one exact tool id belongs to agent collaboration.
// isCollaborationToolName 判断精确工具标识是否属于代理协作工具。
export function isCollaborationToolName(toolName?: string | null): boolean {
  const normalized = toolName?.trim().toLowerCase();
  return Boolean(normalized && COLLABORATION_TOOL_NAMES.has(normalized));
}

// isSubagentFeedbackMetadata returns whether metadata belongs to a child-agent feedback projection.
// isSubagentFeedbackMetadata 判断元数据是否属于智能体反馈投影。
export function isSubagentFeedbackMetadata(metadata: unknown): boolean {
  const normalized = metadataString(metadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ])?.trim().toLowerCase();
  return normalized === "subagent_feedback";
}

// subagentTaskIdFromToolMetadata extracts the stable child subagent task id from host task metadata.
// subagentTaskIdFromToolMetadata 从宿主任务元数据中提取稳定的子代理任务标识。
export function subagentTaskIdFromToolMetadata(metadata: unknown): string | null {
  const hostKind = metadataString(metadata, ["host_result.kind", "hostResult.kind", "kind"]);
  if (isSubagentHostKind(hostKind)) {
    return metadataString(metadata, [
      "host_result.payload.subagent_task_id",
      "hostResult.payload.subagentTaskId",
      "payload.subagent_task_id",
      "payload.subagentTaskId",
      "subagent_task_id",
      "subagentTaskId",
    ]);
  }
  return metadataString(metadata, [
    "subagent_task_id",
    "subagentTaskId",
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
      count: numberMetadata(stats.provider_call_count ?? stats.providerCallCount).toLocaleString("en-US"),
    }),
    formatTurnStatsTranslation(translate, "conversation.stats.toolCalls", {
      count: numberMetadata(stats.tool_call_count ?? stats.toolCallCount).toLocaleString("en-US"),
    }),
  ];
  const groupParts = turnStatsUsageGroupSummaries(stats, translate);
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  if (groupParts.length > 0) {
    parts.push(...groupParts);
  } else if (totalTokens > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.totalTokens", {
      count: formatTokenCount(totalTokens),
    }));
  }
  const cachedTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  if (cachedTokens > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.cachedTokens", {
      count: formatTokenCount(cachedTokens),
    }));
  }
  // Historical turn-wide rates cannot be presented as measured token generation speeds.
  // 历史整轮速率不能作为实测 Token 生成速度显示。
  const outputDurationMs = numericMetadata(stats.output_duration_ms);
  const speed = outputDurationMs !== null && outputDurationMs > 0 ? numericMetadata(stats.output_tokens_per_second) : null;
  if (speed && Number.isFinite(speed) && speed > 0) {
    parts.push(formatTurnStatsTranslation(translate, "conversation.stats.outputSpeed", {
      speed: formatTokenSpeedValue(speed),
    }));
  }
  return parts.join(" · ");
}

// turnStatsUsageGroupSummaries formats first-class regular/agent/compaction usage groups.
// turnStatsUsageGroupSummaries 格式化一等常规、智能体与压缩用量分组。
function turnStatsUsageGroupSummaries(
  // Structured turn-stats object received from metadata.
  // 从元数据中收到的结构化轮次统计对象。
  stats: Record<string, unknown>,
  // Translation helper.
  // 翻译辅助函数。
  translate: (key: TranslationKey) => string,
): string[] {
  const groupsValue = stats.usage_groups ?? stats.usageGroups;
  if (!Array.isArray(groupsValue)) {
    return [];
  }
  const summaries: string[] = [];
  for (const group of groupsValue) {
    if (!group || typeof group !== "object") {
      continue;
    }
    const groupRecord = group as Record<string, unknown>;
    const totalTokens = numberMetadata(groupRecord.total_tokens ?? groupRecord.totalTokens);
    if (totalTokens <= 0) {
      continue;
    }
    const key = turnStatsUsageGroupTranslationKey(groupRecord.kind);
    if (!key) {
      continue;
    }
    const displayKey = key === "conversation.stats.group.regular"
      ? "conversation.stats.totalTokens"
      : key;
    summaries.push(formatTurnStatsTranslation(translate, displayKey, {
      count: formatTokenCount(totalTokens),
    }));
  }
  return summaries;
}

// turnStatsUsageGroupTranslationKey maps serialized usage group kinds to label keys.
// turnStatsUsageGroupTranslationKey 将序列化用量分组类型映射为标签键。
function turnStatsUsageGroupTranslationKey(
  // Serialized group kind from the backend model.
  // 后端模型中的序列化分组类型。
  kind: unknown,
): TranslationKey | null {
  if (typeof kind !== "string") {
    return null;
  }
  switch (kind.trim().toLowerCase()) {
    case "regular":
      return "conversation.stats.group.regular";
    case "subagent":
      return "conversation.stats.group.subagent";
    case "context_compaction":
    case "contextcompaction":
      return "conversation.stats.group.contextCompaction";
    default:
      return null;
  }
}

// isSubagentToolName returns whether one tool name belongs to the built-in subagent tool family.
// isSubagentToolName 判断单个工具名是否属于内置子代理工具族。
export function isSubagentToolName(toolName?: string | null): boolean {
  const normalized = toolName?.trim().toLowerCase();
  return normalized === "spawn_agent"
    || normalized === "agent_orchestration_info"
    || normalized === "send_message"
    || normalized === "wait_agent"
    || normalized === "list_agents"
    || normalized === "interrupt_agent"
    || normalized === "close_agent"
    || normalized === "subagent_feedback";
}

// subagentToolDisplayName extracts the visible child-agent name from tool metadata.
// subagentToolDisplayName 从工具元数据中提取可见的子代理名称。
export function subagentToolDisplayName(metadata: unknown, toolName?: string | null): string | null {
  const metadataToolName = toolName ?? metadataString(metadata, [
    "tool_name",
    "host_result.tool_name",
    "hostResult.toolName",
  ]);
  const subagentTaskId = subagentTaskIdFromToolMetadata(metadata);
  if (!isSubagentToolName(metadataToolName) && !subagentTaskId) {
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
    case "conversation.stats.cachedTokensCompact":
      return "Cache {count} tokens";
    case "conversation.stats.outputSpeed":
      return "{speed} tokens/s";
    case "conversation.stats.group.regular":
      return "Regular {count} tokens";
    case "conversation.stats.group.subagent":
      return "Agent {count} tokens";
    case "conversation.stats.group.contextCompaction":
      return "Compaction {count} tokens";
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

// normalizeToolDisplayState maps provider and host state words into WebUI display states.
// normalizeToolDisplayState 将 Provider 与宿主状态词映射为 WebUI 展示状态。
export function normalizeToolDisplayState(status?: string | null): string | undefined {
  if (!status?.trim()) {
    return undefined;
  }
  const normalized = status.trim().toLowerCase();
  if (normalized === "failed" || normalized === "error" || normalized === "errored" || normalized === "failure" || normalized === "invalid" || normalized === "rejected") {
    return "failed";
  }
  if (normalized === "completed" || normalized === "complete" || normalized === "success" || normalized === "succeeded") {
    return "completed";
  }
  if (normalized === "stopped" || normalized === "cancelled" || normalized === "canceled" || normalized === "aborted" || normalized === "superseded") {
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
  if (attributes.tool_evidence === "true" || attributes.tool_call_id || attributes.command_session_id || attributes.subagent_task_id) {
    return true;
  }
  const hostKind = metadataString(message.metadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && TOOL_EVIDENCE_HOST_KINDS.has(hostKind)) {
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
  if (attributes.tool_evidence === "true" || attributes.tool_call_id || attributes.command_session_id || attributes.subagent_task_id) {
    return true;
  }
  const hostKind = metadataString(rawMetadata, [
    "host_result.kind",
    "hostResult.kind",
    "kind",
  ]);
  if (hostKind && TOOL_EVIDENCE_HOST_KINDS.has(hostKind)) {
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
    metadataValue(metadata, ["host_result", "hostResult"]),
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
