import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { TranslationKey } from "@/i18n-messages";
import type { ToolRequestProgressSnapshot } from "@/types";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";
import {
  isCollaborationToolName,
  isSubagentToolName,
  normalizeToolDisplayState,
  subagentToolDisplayName,
} from "./conversation-tool-metadata";

// ToolRequestProgress describes safe model-side argument construction metadata retained on one tool card.
// ToolRequestProgress 描述保留在单张工具卡片上的安全模型侧参数构造元数据。
export interface ToolRequestProgress {
  // Stable request-stream identity used before host execution begins.
  // 宿主执行开始前使用的稳定请求流标识。
  requestStreamId: string;
  // Current request formation state.
  // 当前请求形成状态。
  state: "receiving" | "arguments_assembled" | "ready";
  // Exact Unicode character count received for the argument document.
  // 参数文档已接收的精确 Unicode 字符数量。
  receivedCharacters: number;
  // Exact UTF-8 byte count received for the argument document.
  // 参数文档已接收的精确 UTF-8 字节数量。
  receivedBytes: number;
  // Latest monotonic request revision.
  // 最近一次单调请求修订号。
  revision: number;
  // Optional parser-sanitized preview metadata.
  // 可选的解析器净化预览元数据。
  preview: Record<string, unknown> | null;
}

// ToolExecutionSourceKind names the stable runtime-owned source projected with ToolStarted.
// ToolExecutionSourceKind 命名随 ToolStarted 投影的稳定运行时工具来源。
export type ToolExecutionSourceKind =
  | "native"
  | "vulcan_managed"
  | "vulcan_plugin"
  | "luaskills"
  | "mcp"
  | "ai_host_binding"
  | "unregistered";

// toolExecutionSummary builds the compact transcript row summary for one tool execution.
// toolExecutionSummary 为单次工具执行构建紧凑转录行摘要。
export function toolExecutionSummary(blocks: ToolExecutionConversationBlock[], sessionRunning: boolean, t?: (key: TranslationKey) => string): string {
  const commandBlock = blocks.find((block) => block.kind === "command");
  const toolName = toolExecutionName(blocks);
  const subagentLabel = toolExecutionSubagentLabel(blocks, t);
  const displayState = toolExecutionDisplayState(blocks, sessionRunning);
  const statusLabel = toolDisplayStateLabel(displayState, t);
  const identifier = toolExecutionIdentifier(blocks);
  const errorPreview = displayState === "failed" ? toolExecutionErrorPreview(blocks) : null;
  const requestProgress = toolRequestProgressForDisplay(toolRequestProgress(blocks), displayState);
  if (commandBlock && !toolName) {
    return commandBlock.summary.toLowerCase().includes("command")
      ? commandBlock.summary
      : `${t ? t("conversation.block.command") : "Command session"} · ${statusLabel}`;
  }

  if (subagentLabel) {
    return `${subagentExecutionActionLabel(t)} · ${subagentLabel} · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }

  if (toolName) {
    if (requestProgress) {
      return `${t ? t("conversation.block.toolCall") : "Tool call"} · ${toolName} · ${toolRequestProgressLabel(requestProgress, t)}`;
    }
    return `${t ? t("conversation.block.toolCall") : "Tool call"} · ${toolName} · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }
  return `${t ? t("conversation.block.toolCall") : "Tool execution"} · ${statusLabel}${identifier ? ` · ${identifier}` : ""}${errorPreview ? ` · ${errorPreview}` : ""}`;
}

// buildToolRawInvocationFallback creates the best explicit legacy envelope available from visible records.
// buildToolRawInvocationFallback 根据可见记录创建当前可获得的最佳显式旧版信封。
export function buildToolRawInvocationFallback(
  blocks: ToolExecutionConversationBlock[],
  displayState: "failed" | "completed" | "running" | "stopped",
): Record<string, unknown> {
  // toolName is the exact visible tool identity retained by the merged row.
  // toolName 是合并行保留的精确可见工具身份。
  const toolName = toolExecutionName(blocks);
  // callId is the stable invocation identity when legacy records retained it.
  // callId 是旧记录保留时可用的稳定调用身份。
  const callId = blocks.map((block) => block.toolCallId).find(Boolean) ?? null;
  // argumentsValue is the best visible argument document available to legacy sessions.
  // argumentsValue 是旧会话可获得的最佳可见参数文档。
  const argumentsValue = blocks
    .map((block) => metadataValue(block.rawMetadata, ["arguments"]))
    .find((value) => value !== null && value !== undefined);
  // output joins only result-bearing visible blocks into the partial fallback.
  // output 仅把承载结果的可见块合并到部分兜底信封。
  const output = blocks
    .filter((block) => block.kind === "tool-result" || block.kind === "command" || block.kind === "diff")
    .map((block) => block.body)
    .filter((body) => body.trim())
    .join("\n\n");
  return {
    version: 1,
    completeness: "legacy_partial",
    callId,
    invocation: {
      tool: toolName,
      arguments: argumentsValue ?? null,
    },
    execution: {
      status: displayState,
    },
    result: output || null,
  };
}

// toolRequestProgress resolves the newest safe argument-construction snapshot from one merged tool row.
// toolRequestProgress 从一条合并工具行中解析最新的安全参数构造快照。
export function toolRequestProgress(blocks: ToolExecutionConversationBlock[]): ToolRequestProgress | null {
  let latest: ToolRequestProgress | null = null;
  for (const block of blocks) {
    const requestStreamId = metadataString(block.rawMetadata, ["request_stream_id"]);
    const state = metadataString(block.rawMetadata, ["tool_display_state"]);
    if (
      !requestStreamId
      || (state !== "receiving" && state !== "arguments_assembled" && state !== "ready")
    ) {
      continue;
    }
    const receivedCharacters = numericMetadata(metadataValue(block.rawMetadata, ["received_characters"]));
    const receivedBytes = numericMetadata(metadataValue(block.rawMetadata, ["received_bytes"]));
    const revision = numericMetadata(metadataValue(block.rawMetadata, ["request_revision"]));
    if (receivedCharacters === null || receivedBytes === null || revision === null) {
      continue;
    }
    const snapshot: ToolRequestProgress = {
      requestStreamId,
      state,
      receivedCharacters: Math.max(0, receivedCharacters),
      receivedBytes: Math.max(0, receivedBytes),
      revision,
      preview: objectMetadata(metadataValue(block.rawMetadata, ["preview"])),
    };
    if (!latest || snapshot.revision > latest.revision) {
      latest = snapshot;
    }
  }
  return latest;
}

// toolRequestStreamId resolves the newest retained request identity even after execution overwrites its display state.
// toolRequestStreamId 即使执行状态覆盖展示状态后，也会解析最新保留的请求身份。
export function toolRequestStreamId(blocks: ToolExecutionConversationBlock[]): string | null {
  // latest tracks the exact newest coordinator revision retained on the merged tool row.
  // latest 跟踪合并工具行上保留的精确最新协调器修订。
  let latest: { requestStreamId: string; revision: number } | null = null;
  for (const block of blocks) {
    // requestStreamId and revision form the closed identity ordering contract.
    // requestStreamId 与 revision 构成封闭的身份排序契约。
    const requestStreamId = metadataString(block.rawMetadata, ["request_stream_id"]);
    const revision = numericMetadata(metadataValue(block.rawMetadata, ["request_revision"]));
    if (!requestStreamId || revision === null) {
      continue;
    }
    if (!latest || revision > latest.revision) {
      latest = { requestStreamId, revision };
    }
  }
  return latest?.requestStreamId ?? null;
}

// toolRequestProgressForDisplay keeps request formation visible only while it owns the tool card status.
// toolRequestProgressForDisplay 仅在请求构造仍拥有工具卡状态时显示请求进度。
export function toolRequestProgressForDisplay(
  progress: ToolRequestProgress | null,
  displayState: "failed" | "completed" | "running" | "stopped",
): ToolRequestProgress | null {
  if (!progress) {
    return null;
  }
  return displayState === "running" ? progress : null;
}

// liveToolRequestProgress projects one external Store snapshot into the existing card model.
// liveToolRequestProgress 将单条外部 Store 快照投影为既有卡片模型。
// Parameters: snapshot is the exact safe backend request-progress contract.
// 参数：snapshot 是精确的后端安全请求进度契约。
// Returns: one immutable card progress model.
// 返回值：一个不可变卡片进度模型。
export function liveToolRequestProgress(
  snapshot: ToolRequestProgressSnapshot,
): ToolRequestProgress {
  return {
    requestStreamId: snapshot.requestStreamId,
    state: snapshot.state,
    receivedCharacters: Math.max(0, snapshot.receivedCharacters),
    receivedBytes: Math.max(0, snapshot.receivedBytes),
    revision: snapshot.requestRevision,
    preview: snapshot.preview ?? null,
  };
}

// toolExecutionDetailLoadEnabled accepts detail hydration only after an authoritative terminal result.
// toolExecutionDetailLoadEnabled 仅在权威终态结果出现后允许详情水合。
// Parameters: displayState is the durable execution state, requestProgress is any active formation
// snapshot, and detailRef is the host-owned terminal detail identity.
// 参数：displayState 是持久执行状态，requestProgress 是任意活动构造快照，detailRef 是宿主持有的
// 终态详情身份。
// Returns: true only when an actual terminal detail lookup is valid.
// 返回值：仅在真实终态详情查询有效时返回 true。
export function toolExecutionDetailLoadEnabled(
  displayState: "failed" | "completed" | "running" | "stopped",
  requestProgress: ToolRequestProgress | null,
  detailRef: Omit<ConversationDetailRef, "sessionId"> | null,
): boolean {
  return displayState !== "running"
    && requestProgress === null
    && detailRef !== null;
}

// toolRequestProgressLabel formats the stable request phase together with its exact live counters.
// toolRequestProgressLabel 将稳定请求阶段与精确实时计数一起格式化。
export function toolRequestProgressLabel(
  progress: ToolRequestProgress,
  t?: (key: TranslationKey) => string,
): string {
  return `${toolRequestProgressStateLabel(progress, t)}(${progress.receivedBytes}B)`;
}

// toolRequestProgressStateLabel formats the request-receiving state used to override execution states.
// toolRequestProgressStateLabel 格式化用于覆盖执行状态的请求接收状态。
//
// Parameters:
// `progress` is the latest exact request snapshot.
// `t` optionally resolves localized copy.
// 参数：
// `progress` 是最新的精确请求快照。
// `t` 可选地解析本地化文案。
//
// Returns:
// A localized request state label that remains distinct from actual tool execution.
// 返回值：
// 返回与真实工具执行明确区分的本地化请求状态文案。
export function toolRequestProgressStateLabel(
  progress: ToolRequestProgress,
  t?: (key: TranslationKey) => string,
): string {
  void progress;
  return t ? t("conversation.toolRequest.receiving") : "Creating";
}

// toolExecutionName resolves the best human-visible tool name across merged call and result blocks.
// toolExecutionName 从合并后的调用与返回块中解析最佳人类可见工具名。
export function toolExecutionName(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const name = block.toolName;
    if (name) {
      return name;
    }
  }
  return null;
}

// toolExecutionSubagentName resolves the visible child-agent name for built-in subagent tool rows.
// toolExecutionSubagentName 为内置子代理工具行解析可见子代理名称。
export function toolExecutionSubagentName(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const name = block.subagentName ?? subagentToolDisplayName(block.rawMetadata, block.toolName);
    if (name) {
      return name;
    }
  }
  return null;
}

// toolExecutionSubagentLabel resolves the child-agent label used in compact transcript summaries.
// toolExecutionSubagentLabel 解析紧凑转录摘要中使用的子代理标签。
export function toolExecutionSubagentLabel(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string | null {
  const name = toolExecutionSubagentName(blocks);
  if (name) {
    return name;
  }
  return blocks.some((block) => isSubagentToolName(block.toolName) || isCollaborationToolName(block.toolName) || Boolean(block.subagentTaskId))
    ? (t ? t("conversation.subagent.schedule") : "Subagent schedule")
    : null;
}

// subagentExecutionActionLabel returns the stable label for child-agent creation rows.
// subagentExecutionActionLabel 返回子代理创建行的稳定标签。
function subagentExecutionActionLabel(t?: (key: TranslationKey) => string): string {
  return t ? t("conversation.subagent.call") : "Create agent";
}

// toolExecutionIdentifier resolves the most useful fallback identifier for a tool row.
// toolExecutionIdentifier 为工具行解析最有用的兜底标识。
export function toolExecutionIdentifier(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const identifier = block.toolCallId
      ?? block.commandSessionId
      ?? block.subagentTaskId;
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

// toolExecutionSourceKind resolves the authoritative runtime source retained by one merged tool row.
// toolExecutionSourceKind 解析单条合并工具行保留的权威运行时来源。
// Parameters: blocks are the visible records merged into one execution card.
// 参数：blocks 是合并为一张执行卡片的可见记录。
// Returns: a validated source key, or null for legacy records that predate source projection.
// 返回值：返回经过校验的来源键；早于来源投影的旧记录返回 null。
export function toolExecutionSourceKind(
  blocks: ToolExecutionConversationBlock[],
): ToolExecutionSourceKind | null {
  for (const block of blocks) {
    const source = metadataString(block.rawMetadata, ["tool_source"]);
    if (
      source === "native"
      || source === "vulcan_managed"
      || source === "vulcan_plugin"
      || source === "luaskills"
      || source === "mcp"
      || source === "ai_host_binding"
      || source === "unregistered"
    ) {
      return source;
    }
  }
  return null;
}

// toolExecutionAllowsRawInvocation returns whether a non-native terminal card may expose raw details.
// toolExecutionAllowsRawInvocation 返回非原生终态卡片是否可以暴露原始详情。
// Parameters: blocks are the exact records whose runtime source is inspected.
// 参数：blocks 是需要检查运行时来源的精确记录。
// Returns: true only for an explicitly projected non-native or unregistered source.
// 返回值：仅当来源被明确投影为非原生或未注册时返回 true。
export function toolExecutionAllowsRawInvocation(
  blocks: ToolExecutionConversationBlock[],
): boolean {
  const source = toolExecutionSourceKind(blocks);
  return source !== null && source !== "native";
}

// toolExecutionSupportsDetailLookup returns whether the row owns a hidden reference or a legacy terminal result.
// toolExecutionSupportsDetailLookup 返回当前行是否持有隐藏引用或旧版终态结果。
export function toolExecutionSupportsDetailLookup(blocks: ToolExecutionConversationBlock[]): boolean {
  return toolExecutionDetailRef(blocks) !== null
    || blocks.some((block) => block.kind === "tool-result" || (block.kind === "diff" && block.body.trim().length > 0));
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
    const hostKind = metadataString(block.rawMetadata, ["host_result.kind", "hostResult.kind"]);
    const hostErrorPaths = hostKind === "tool_error"
      ? ["host_result.payload.message", "hostResult.payload.message", "host_result.payload.error", "hostResult.payload.error", "host_result.payload.reason", "hostResult.payload.reason"]
      : [];
    const metadataError = metadataString(block.rawMetadata, [
      "result.error",
      "result.reason",
      ...hostErrorPaths,
    ]);
    if (metadataError) {
      return compactToolExecutionErrorPreview(metadataError);
    }
  }
  const failedBlock = blocks.find((block) => normalizeToolDisplayState(block.status) === "failed");
  const preview = failedBlock?.body || failedBlock?.summary;
  return preview ? compactToolExecutionErrorPreview(preview) : null;
}

// preferredSubagentTaskDisplayBlock keeps routed subagent-task rows anchored to the newest terminal metadata.
// preferredSubagentTaskDisplayBlock 让已路由子代理任务行优先使用最新终态元数据作为展示锚点。
export function preferredSubagentTaskDisplayBlock(blocks: ToolExecutionConversationBlock[]): ToolExecutionConversationBlock | null {
  const taskBlocks = blocks.filter((block) => block.subagentTaskId);
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
  const subagentTaskState = subagentTaskExecutionDisplayState(blocks);
  if (subagentTaskState) {
    return subagentTaskState;
  }
  // terminalBoundaryStates give actual results or hidden details precedence over stale request snapshots.
  // terminalBoundaryStates 让真实结果或隐藏详情优先于残留的请求快照。
  const terminalBoundaryStates = blocks.map((block) => {
    // metadataState reads the runtime-owned lifecycle before inspecting display-derived status.
    // metadataState 在检查展示派生状态前读取运行时持有的生命周期。
    const metadataState = normalizeToolDisplayState(metadataString(block.rawMetadata, ["tool_display_state"]));
    if (metadataState === "failed" || metadataState === "stopped" || metadataState === "completed") {
      return metadataState;
    }
    if (block.kind === "tool-result" || toolExecutionDetailRef([block])) {
      // blockState preserves explicit failure and stop states on legacy terminal result rows.
      // blockState 保留旧版终态结果行上的显式失败与停止状态。
      const blockState = normalizeToolDisplayState(block.status);
      return blockState === "failed" || blockState === "stopped" ? blockState : "completed";
    }
    return null;
  }).filter((state): state is "failed" | "completed" | "stopped" => state !== null);
  if (terminalBoundaryStates.includes("failed")) {
    return "failed";
  }
  if (terminalBoundaryStates.includes("stopped")) {
    return "stopped";
  }
  if (terminalBoundaryStates.includes("completed")) {
    return "completed";
  }
  // requestFormationIsAuthoritative prevents derived block status from skipping ToolStarted.
  // requestFormationIsAuthoritative 防止派生块状态绕过 ToolStarted 边界。
  const requestFormationIsAuthoritative = blocks.some((block) => {
    // state is the exact request-formation boundary retained on this block.
    // state 是当前块保留的精确请求构造边界。
    const state = metadataString(block.rawMetadata, ["tool_display_state"]);
    return state === "receiving" || state === "arguments_assembled" || state === "ready";
  });
  if (requestFormationIsAuthoritative) {
    return "running";
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
  if (blocks.some((block) => block.kind === "command")) {
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
export function toolDisplayStateLabel(status: "failed" | "completed" | "running" | string, t?: (key: TranslationKey) => string): string {
  if (status === "failed") return t ? t("toolPresentation.command.failed") : "Failed";
  if (status === "completed") return t ? t("toolPresentation.command.completed") : "Completed";
  if (status === "stopped") return t ? t("toolPresentation.command.stopped") : "Stopped";
  return t ? t("toolPresentation.command.running") : "Running";
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

// subagentTaskExecutionDisplayState resolves the visible state for routed subagent-task rows from explicit task metadata.
// subagentTaskExecutionDisplayState 从显式任务元数据解析已路由子代理任务行的可见状态。
function subagentTaskExecutionDisplayState(blocks: ToolExecutionConversationBlock[]): "failed" | "completed" | "running" | "stopped" | null {
  const taskBlocks = blocks.filter((block) => block.subagentTaskId);
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
  // A returned orchestration result completes the tool invocation even when its payload reports that the spawned child task is still running.
  // 编排工具结果一旦返回即代表本次工具调用完成，即使其载荷仍报告新建子任务处于运行状态。
  if (taskBlocks.some((block) => block.kind === "tool-result")) {
    return "completed";
  }
  if (states.includes("running")) {
    return "running";
  }
  return null;
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

// objectMetadata narrows unknown JSON values to plain objects.
// objectMetadata 将未知 JSON 值收窄为普通对象。
function objectMetadata(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
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
