import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { TranslationKey } from "@/i18n-messages";
import type { ToolRequestProgressSnapshot } from "@/types";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";
import { formatDurationSeconds } from "@/lib/usage-format";
import {
  isCollaborationToolName,
  isSubagentFeedbackMetadata,
  isSubagentHostKind,
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

// buildToolExecutionDetailBody builds the expensive modal body only when the user opens details.
// buildToolExecutionDetailBody 仅在用户打开详情时构建成本较高的弹窗正文。
export function buildToolExecutionDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const subagentBody = buildSubagentToolExecutionDetailBody(blocks, t);
  if (subagentBody) {
    return subagentBody;
  }
  return blocks
    .map((block) => {
      const sections = [`## ${toolBlockTitle(block, t)}`];
      const argumentsValue = metadataValue(block.rawMetadata, ["arguments"]);
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
  return `${toolRequestProgressStateLabel(progress, t)} · ${toolRequestStreamMetricsLabel(progress, t)}`;
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
  return t ? t("conversation.toolRequest.receiving") : "Requesting";
}

// toolRequestStreamMetricsLabel formats exact character and UTF-8 byte progress for one request stream.
// toolRequestStreamMetricsLabel 为单条请求流格式化精确字符数与 UTF-8 字节进度。
//
// Parameters: progress is the latest request snapshot and t resolves localized unit text.
// 参数：progress 是最新请求快照，t 用于解析本地化单位文案。
//
// Returns: a compact live metric string whose value changes with every accepted request revision.
// 返回值：返回紧凑实时指标文本，其值会随每个已接纳请求修订而变化。
export function toolRequestStreamMetricsLabel(
  progress: ToolRequestProgress,
  t?: (key: TranslationKey) => string,
): string {
  const characterLabel = `${progress.receivedCharacters.toLocaleString()} ${t ? t("conversation.toolRequest.characters") : "characters"}`;
  return `${characterLabel} · ${formatToolRequestBytes(progress.receivedBytes)}`;
}

// formatToolRequestBytes formats one exact non-negative UTF-8 byte count for inline progress.
// formatToolRequestBytes 为内联进度格式化一项精确非负 UTF-8 字节数。
//
// Parameters: bytes is the exact count supplied by the request-stream coordinator.
// 参数：bytes 是请求流协调器提供的精确计数。
//
// Returns: a compact binary-unit label without changing the underlying count.
// 返回值：返回不改变底层计数的紧凑二进制单位文案。
function formatToolRequestBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes.toLocaleString()} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

// buildSubagentToolExecutionDetailBody builds a human-readable detail body for built-in subagent orchestration tools.
// buildSubagentToolExecutionDetailBody 为内置子代理调度工具构建面向人的详情正文。
function buildSubagentToolExecutionDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string | null {
  const toolName = toolExecutionName(blocks);
  const hostKind = firstMetadataString(blocks, ["host_result.kind", "hostResult.kind", "kind"]);
  if (toolName === "wait_agents_result") {
    return buildSubagentWaitDetailBody(blocks, t);
  }
  if (isCollaborationToolName(toolName)) {
    return buildCollaborationToolExecutionDetailBody(blocks);
  }
  if (!isSubagentToolName(toolName) && !isSubagentHostKind(hostKind)) {
    return null;
  }
  if (toolName === "send_message" || hostKind === "subagent_message") {
    return buildSubagentMessageDetailBody(blocks, t);
  }
  if (toolName === "wait_agent" || hostKind === "subagent_wait") {
    return buildSubagentWaitDetailBody(blocks, t);
  }
  if (toolName === "subagent_feedback" || blocks.some((block) => isSubagentFeedbackMetadata(block.rawMetadata))) {
    return buildSubagentFeedbackDetailBody(blocks, t);
  }
  if (toolName === "agent_orchestration_info") {
    return buildSubagentTemplateListDetailBody(blocks, t);
  }
  if (toolName === "list_agents" || hostKind === "subagent_status") {
    return buildSubagentStatusListDetailBody(blocks, t);
  }
  const controlOperation = hostKind === "subagent_control" ? subagentControlDetailOperation(blocks) : null;
  if (toolName === "close_agent" || controlOperation === "close") {
    return buildSubagentCloseDetailBody(blocks, t);
  }
  if (toolName === "interrupt_agent" || controlOperation === "interrupt") {
    return buildSubagentInterruptDetailBody(blocks, t);
  }
  return buildSubagentSpawnDetailBody(blocks, t);
}

// buildCollaborationToolExecutionDetailBody renders exact collaboration arguments and opaque output text.
// buildCollaborationToolExecutionDetailBody 渲染精确协作参数与不透明工具输出正文。
function buildCollaborationToolExecutionDetailBody(blocks: ToolExecutionConversationBlock[]): string {
  const toolName = toolExecutionName(blocks) ?? "collaboration";
  const argumentsValue = blocks
    .map((block) => metadataValue(block.rawMetadata, ["arguments"]))
    .find((value) => value !== null && value !== undefined);
  const sections = [`## ${toolName}`];
  if (argumentsValue !== undefined) {
    sections.push(`### Arguments\n\n\`\`\`json\n${JSON.stringify(argumentsValue, null, 2)}\n\`\`\``);
  }
  const body = blocks.map((block) => block.body.trim()).find(Boolean);
  if (body) {
    sections.push(`### Tool output\n\n${body}`);
  }
  return sections.join("\n\n");
}

// buildSubagentMessageDetailBody renders send_message as a compact inline record.
// buildSubagentMessageDetailBody 将 send_message 渲染为紧凑内联记录。
//
// Parameters:
// blocks is the merged tool execution row.
// t is the optional translation helper.
// 参数：
// blocks 是合并后的工具执行行。
// t 是可选翻译辅助函数。
//
// Returns:
// A Markdown detail body without raw JSON.
// 返回值：
// 不包含原始 JSON 的 Markdown 详情正文。
function buildSubagentMessageDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const target = firstMetadataString(blocks, [
    "arguments.target",
    "host_result.payload.target",
    "hostResult.payload.target",
    "payload.target",
  ]);
  const message = firstMetadataString(blocks, [
    "arguments.message",
    "host_result.payload.message",
    "hostResult.payload.message",
    "payload.message",
  ]) ?? "";
  const deliveryMode = firstMetadataString(blocks, [
    "arguments.delivery_mode",
    "arguments.deliveryMode",
    "host_result.payload.delivery_mode",
    "hostResult.payload.deliveryMode",
    "payload.delivery_mode",
    "payload.deliveryMode",
  ]);
  return [
    `## ${text(t, "conversation.subagent.detail.message", "Subagent message")}`,
    target ? detailLine(text(t, "conversation.subagent.detail.target", "Target"), target) : null,
    deliveryMode ? detailLine(text(t, "conversation.subagent.detail.deliveryMode", "Delivery mode"), deliveryMode) : null,
    message
      ? detailLine(text(t, "conversation.subagent.detail.message", "Subagent message"), message)
      : detailLine(text(t, "conversation.subagent.detail.message", "Subagent message"), text(t, "conversation.subagent.detail.empty", "No content.")),
  ].filter(Boolean).join("\n\n");
}

// buildSubagentSpawnDetailBody renders only the one-time task input for subagent creation records.
// buildSubagentSpawnDetailBody 仅渲染子代理创建记录的本次任务输入。
function buildSubagentSpawnDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const name = toolExecutionSubagentName(blocks) ?? text(t, "conversation.subagent.schedule", "Subagent schedule");
  const prompt = firstMetadataString(blocks, [
    "arguments.message",
    "arguments.prompt",
    "host_result.payload.prompt",
    "hostResult.payload.prompt",
    "payload.prompt",
  ]) ?? "";
  const description = firstMetadataString(blocks, [
    "arguments.description",
    "host_result.payload.description",
    "hostResult.payload.description",
    "payload.description",
  ]);
  return [
    `## ${text(t, "conversation.subagent.detail.createdTask", "Created agent task")}`,
    detailLine(text(t, "conversation.subagent.detail.name", "Name"), name),
    description ? detailLine(text(t, "conversation.subagent.detail.description", "Description"), description) : null,
    prompt
      ? detailLine(text(t, "conversation.subagent.detail.prompt", "Prompt"), prompt)
      : detailLine(text(t, "conversation.subagent.detail.prompt", "Prompt"), text(t, "conversation.subagent.detail.empty", "No content.")),
  ].filter(Boolean).join("\n\n");
}

// buildSubagentWaitDetailBody renders wait_agent results as a mailbox wait summary.
// buildSubagentWaitDetailBody 将 wait_agent 结果渲染为 mailbox 等待摘要。
function buildSubagentWaitDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const timeoutMs = numberMetadata(payload?.timeout_ms ?? payload?.timeoutMs);
  const timedOut = booleanMetadata(payload?.timed_out ?? payload?.timedOut);
  const interrupted = booleanMetadata(payload?.interrupted);
  const result = stringMetadata(payload?.result);
  const agents = arrayMetadata(payload, "agents").map(objectMetadata).filter((agent): agent is Record<string, unknown> => Boolean(agent));
  const agentSections = agents.map((agent, index) => {
    const name = stringMetadata(agent.display_title ?? agent.displayTitle ?? agent.agent_id ?? agent.agentId ?? agent.subagent_task_id ?? agent.subagentTaskId)
      ?? `${text(t, "conversation.subagent.detail.agent", "Agent")} ${index + 1}`;
    const status = stringMetadata(agent.status ?? agent.kind);
    const agentResult = stringMetadata(agent.result ?? agent.error);
    return [
      `### ${name}`,
      status ? detailLine(text(t, "conversation.subagent.detail.status", "Status"), status) : null,
      agentResult ? `#### ${text(t, "conversation.subagent.detail.result", "Result")}\n\n${fencedText(agentResult)}` : null,
    ].filter(Boolean).join("\n\n");
  });
  return [
    `## ${text(t, "conversation.subagent.detail.wait", "Waiting for subagent message")}`,
    detailLine(
      text(t, "conversation.subagent.detail.status", "Status"),
      subagentWaitStatusText(payload, timedOut, interrupted, t),
    ),
    timeoutMs !== null && timeoutMs > 0
      ? detailLine(text(t, "conversation.subagent.detail.timeout", "Timeout"), formatDurationSeconds(timeoutMs))
      : null,
    result ? `### ${text(t, "conversation.subagent.detail.result", "Result")}\n\n${fencedText(result)}` : null,
    ...agentSections,
  ].filter(Boolean).join("\n\n");
}

// subagentWaitStatusText maps wait_agent activity payloads to accurate user-facing labels.
// subagentWaitStatusText 将 wait_agent 活动载荷映射为准确的用户可见文案。
function subagentWaitStatusText(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
  // Whether the wait call timed out.
  // 等待调用是否超时。
  timedOut: boolean | null,
  // Whether the parent turn interrupted this wait.
  // 父级回合是否打断本次等待。
  interrupted: boolean | null,
  // Optional translation helper.
  // 可选翻译辅助函数。
  t?: (key: TranslationKey) => string,
): string {
  // Unified inbox outcomes are host-owned; delivery receipts are not completion signals.
  // 统一收件结果由宿主拥有，投递回执不是完成信号。
  if (payload?.status === "messages") {
    return text(t, "conversation.subagent.detail.waitCompleted", "Agent message received");
  }
  if (payload?.status === "no_pending_work") {
    return text(t, "conversation.subagent.detail.noPendingWork", "No pending agent work");
  }
  if (payload?.status === "timeout") {
    return text(t, "conversation.subagent.detail.waitTimedOut", "Wait timed out");
  }
  if (timedOut) {
    return text(t, "conversation.subagent.detail.waitTimedOut", "Wait timed out");
  }
  if (interrupted) {
    return text(t, "conversation.subagent.detail.waitInterrupted", "Wait interrupted");
  }
  if (subagentWaitTerminalEventReceived(payload)) {
    return text(t, "conversation.subagent.detail.waitCompleted", "Subagent message received");
  }
  return text(t, "conversation.subagent.detail.waitPending", "Waiting");
}

// subagentWaitTerminalEventReceived checks the wait_agent contract for terminal child events.
// subagentWaitTerminalEventReceived 根据 wait_agent 契约检查是否收到子智能体终态事件。
function subagentWaitTerminalEventReceived(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): boolean {
  const terminalEventCount = numberMetadata(payload?.terminal_event_count ?? payload?.terminalEventCount);
  if (terminalEventCount !== null && terminalEventCount > 0) {
    return true;
  }
  const activityStatus = stringMetadata(payload?.activity_status ?? payload?.activityStatus)?.toLowerCase();
  if (activityStatus === "terminal_update") {
    return true;
  }
  return arrayMetadata(payload, "agents").some((rawAgent) => {
    const agent = objectMetadata(rawAgent);
    return booleanMetadata(agent?.terminal) === true || subagentAgentHasTerminalState(agent);
  });
}

// subagentAgentHasTerminalState recognizes terminal wait_agent rows from compact agent state.
// subagentAgentHasTerminalState 根据紧凑智能体状态识别 wait_agent 终态行。
function subagentAgentHasTerminalState(
  // Parsed agent payload.
  // 已解析的智能体载荷。
  agent: Record<string, unknown> | null,
): boolean {
  const status = stringMetadata(agent?.status ?? agent?.kind ?? agent?.task_status ?? agent?.taskStatus ?? agent?.execution_status ?? agent?.executionStatus)
    ?.toLowerCase();
  return status === "completed" || status === "failed" || status === "stopped" || status === "errored";
}

// buildSubagentFeedbackDetailBody renders a display-only child feedback card body.
// buildSubagentFeedbackDetailBody 渲染仅显示的智能体反馈卡片正文。
function buildSubagentFeedbackDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const result = firstMetadataString(blocks, [
    "host_result.payload.result",
    "hostResult.payload.result",
    "payload.result",
    "result",
    "host_result.payload.message",
    "hostResult.payload.message",
    "payload.message",
    "message",
  ]);
  const name = firstMetadataString(blocks, [
    "host_result.payload.subagent",
    "hostResult.payload.subagent",
    "payload.subagent",
    "subagent",
    "host_result.payload.task_id",
    "hostResult.payload.taskId",
    "payload.task_id",
    "payload.taskId",
  ]);
  return [
    `## ${text(t, "conversation.subagent.detail.feedback", "Agent returned result")}`,
    name ? detailLine(text(t, "conversation.subagent.detail.name", "Name"), name) : null,
    result
      ? `### ${text(t, "conversation.subagent.detail.result", "Result")}\n\n${fencedText(result)}`
      : text(t, "conversation.subagent.detail.noResult", "No result content."),
    payload?.run_state || payload?.runState
      ? detailLine(text(t, "conversation.subagent.detail.status", "Status"), String(payload.run_state ?? payload.runState))
      : null,
  ].filter(Boolean).join("\n\n");
}

// buildSubagentTemplateListDetailBody renders agent template query results.
// buildSubagentTemplateListDetailBody 渲染智能体模板查询结果。
function buildSubagentTemplateListDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  return buildSubagentAgentListDetailBody(
    blocks,
    text(t, "conversation.subagent.detail.templateCollection", "Agent template list"),
    t,
  );
}

// buildSubagentStatusListDetailBody renders open-agent status query results.
// buildSubagentStatusListDetailBody 渲染未关闭智能体状态查询结果。
function buildSubagentStatusListDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  return buildSubagentAgentListDetailBody(
    blocks,
    text(t, "conversation.subagent.detail.statusCollection", "Agent status list"),
    t,
  );
}

// buildSubagentAgentListDetailBody renders one concrete agent-list detail body.
// buildSubagentAgentListDetailBody 渲染一个具体智能体列表详情正文。
function buildSubagentAgentListDetailBody(blocks: ToolExecutionConversationBlock[], title: string, t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const agents = arrayMetadata(payload, "agents").map(objectMetadata).filter((agent): agent is Record<string, unknown> => Boolean(agent));
  const completed = booleanMetadata(payload?.completed);
  const timeoutMs = numberMetadata(payload?.timeout_ms ?? payload?.timeoutMs);
  const message = stringMetadata(payload?.message);
  const result = stringMetadata(payload?.result);
  const header = [
    `## ${title}`,
    detailLine(text(t, "conversation.subagent.detail.count", "Count"), String(agents.length)),
    completed !== null ? detailLine(text(t, "conversation.subagent.detail.completed", "Completed"), booleanText(completed, t)) : null,
    timeoutMs !== null && timeoutMs > 0 ? detailLine(text(t, "conversation.subagent.detail.timeout", "Timeout"), formatDurationSeconds(timeoutMs)) : null,
  ].filter(Boolean);
  if (!agents.length) {
    return [
      ...header,
      message ? detailLine(text(t, "conversation.subagent.detail.note", "Note"), message) : null,
      result ? `### ${text(t, "conversation.subagent.detail.result", "Result")}\n\n${fencedText(result)}` : null,
      !message && !result ? text(t, "conversation.subagent.detail.noAgents", "No visible subagent.") : null,
    ].filter(Boolean).join("\n\n");
  }
  const agentSections = agents.map((agent, index) => {
    const name = stringMetadata(agent.agent_name ?? agent.agentName ?? agent.display_title ?? agent.displayTitle ?? agent.task_name ?? agent.taskName ?? agent.agent_id ?? agent.agentId)
      ?? `${text(t, "conversation.subagent.detail.agent", "Agent")} ${index + 1}`;
    const enabled = booleanMetadata(agent.enabled);
    const status = subagentAgentStatusText(agent)
      ?? stringMetadata(agent.status)
      ?? (enabled !== null ? (enabled ? text(t, "conversation.subagent.enabled", "Enabled") : text(t, "conversation.subagent.disabled", "Disabled")) : null)
      ?? text(t, "conversation.subagent.detail.unknown", "Unknown");
    const result = subagentAgentStatusResult(agent) ?? stringMetadata(agent.result) ?? stringMetadata(agent.error) ?? "";
    const description = stringMetadata(agent.description);
    return [
      `### ${index + 1}. ${name}`,
      detailLine(text(t, "conversation.subagent.detail.status", "Status"), status),
      description ? detailLine(text(t, "conversation.subagent.detail.description", "Description"), description) : null,
      `#### ${text(t, "conversation.subagent.detail.result", "Result")}`,
      result.trim() ? fencedText(result) : text(t, "conversation.subagent.detail.noResult", "No result content."),
    ].filter(Boolean).join("\n\n");
  });
  return [...header, ...agentSections].join("\n\n");
}

// normalizeToolExecutionName converts tool names into a stable comparison identity.
// normalizeToolExecutionName 将工具名转换为稳定比较标识。
function normalizeToolExecutionName(
  // Raw tool name.
  // 原始工具名。
  value: string,
): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "_");
}

// subagentAgentStatusText formats Codex-style agent_status values for detail panels.
// subagentAgentStatusText 为详情面板格式化 Codex 风格的 agent_status 值。
function subagentAgentStatusText(agent: Record<string, unknown>): string | null {
  const status = agent.agent_status ?? agent.agentStatus;
  if (typeof status === "string" && status.trim()) {
    return status.trim();
  }
  const statusObject = objectMetadata(status);
  if (!statusObject) {
    return null;
  }
  if ("completed" in statusObject) {
    return "completed";
  }
  if ("errored" in statusObject) {
    return "errored";
  }
  return null;
}

// subagentAgentStatusResult extracts terminal text from Codex-style agent_status values.
// subagentAgentStatusResult 从 Codex 风格的 agent_status 值中提取终态文本。
function subagentAgentStatusResult(agent: Record<string, unknown>): string | null {
  const statusObject = objectMetadata(agent.agent_status ?? agent.agentStatus);
  return stringMetadata(statusObject?.completed) ?? stringMetadata(statusObject?.errored);
}

// subagentControlDetailOperation resolves legacy control detail rows into concrete tools.
// subagentControlDetailOperation 将旧式控制详情行解析为具体工具。
function subagentControlDetailOperation(
  // Blocks merged into one tool presentation row.
  // 合并为单个工具展示行的块。
  blocks: ToolExecutionConversationBlock[],
): "close" | "interrupt" | null {
  const payload = firstPayloadObject(blocks);
  const payloadTool = normalizeToolExecutionName(stringMetadata(payload?.tool) ?? "");
  if (payloadTool === "close_agent") {
    return "close";
  }
  if (payloadTool === "interrupt_agent") {
    return "interrupt";
  }
  if (
    payload
    && (
      "closed" in payload
      || "closed_count" in payload
      || "closedCount" in payload
      || "closed_agents" in payload
      || "closedAgents" in payload
      || "cleared_message_count" in payload
      || "clearedMessageCount" in payload
    )
  ) {
    return "close";
  }
  if (
    payload
    && (
      "stopped_count" in payload
      || "stoppedCount" in payload
      || "stopped_subagent_task_ids" in payload
      || "stoppedSubagentTaskIds" in payload
    )
  ) {
    return "interrupt";
  }
  return null;
}

// buildSubagentCloseDetailBody renders close-agent results as a compact operation summary.
// buildSubagentCloseDetailBody 将关闭智能体结果渲染为紧凑操作摘要。
function buildSubagentCloseDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const target = firstMetadataString(blocks, ["arguments.task_name", "arguments.task_id", "arguments.target"]);
  const stoppedIds = [
    ...arrayMetadata(payload, "stopped_subagent_task_ids"),
  ].map(stringMetadata).filter((value): value is string => Boolean(value));
  const closedAgents = arrayMetadata(payload, "closed_agents").map(stringMetadata).filter((value): value is string => Boolean(value));
  const closedCount = numberMetadata(payload?.closed_count ?? payload?.closedCount) ?? closedAgents.length;
  const stoppedCount = numberMetadata(payload?.stopped_count ?? payload?.stoppedCount) ?? stoppedIds.length;
  const clearedMessageCount = numberMetadata(payload?.cleared_message_count ?? payload?.clearedMessageCount);
  return [
    `## ${text(t, "conversation.subagent.detail.closeControl", "Close agent")}`,
    target ? detailLine(text(t, "conversation.subagent.detail.target", "Target"), target) : null,
    closedCount > 0 ? detailLine(text(t, "conversation.subagent.detail.closedCount", "Closed count"), String(closedCount)) : null,
    stoppedCount > 0 ? detailLine(text(t, "conversation.subagent.detail.stoppedCount", "Stopped count"), String(stoppedCount)) : null,
    stoppedIds.length ? `${detailLine(text(t, "conversation.subagent.detail.stoppedSessions", "Stopped sessions"), "")}\n${stoppedIds.map((id) => `- ${id}`).join("\n")}` : null,
    closedAgents.length ? `${detailLine(text(t, "conversation.subagent.detail.closedAgents", "Closed agents"), "")}\n${closedAgents.map((id) => `- ${id}`).join("\n")}` : null,
    clearedMessageCount !== null ? detailLine(text(t, "conversation.subagent.detail.clearedMessages", "Cleared messages"), String(clearedMessageCount)) : null,
  ].filter(Boolean).join("\n\n");
}

// buildSubagentInterruptDetailBody renders interrupt-agent results as a compact operation summary.
// buildSubagentInterruptDetailBody 将停止智能体结果渲染为紧凑操作摘要。
function buildSubagentInterruptDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const target = firstMetadataString(blocks, ["arguments.task_name", "arguments.task_id", "arguments.target"]);
  const stoppedIds = [
    ...arrayMetadata(payload, "stopped_subagent_task_ids"),
  ].map(stringMetadata).filter((value): value is string => Boolean(value));
  const stoppedCount = numberMetadata(payload?.stopped_count ?? payload?.stoppedCount) ?? stoppedIds.length;
  return [
    `## ${text(t, "conversation.subagent.detail.interruptControl", "Stop agent")}`,
    target ? detailLine(text(t, "conversation.subagent.detail.target", "Target"), target) : null,
    stoppedCount > 0 ? detailLine(text(t, "conversation.subagent.detail.stoppedCount", "Stopped count"), String(stoppedCount)) : null,
    stoppedIds.length ? `${detailLine(text(t, "conversation.subagent.detail.stoppedSessions", "Stopped sessions"), "")}\n${stoppedIds.map((id) => `- ${id}`).join("\n")}` : null,
  ].filter(Boolean).join("\n\n");
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

// toolBlockTitle returns a localized title for one semantic transcript block.
// toolBlockTitle 返回单个语义转录块的本地化标题。
function toolBlockTitle(block: ToolExecutionConversationBlock, t?: (key: TranslationKey) => string): string {
  const kind = block.kind;
  const toolName = block.toolName;
  const subagentLabel = toolExecutionSubagentLabel([block], t);
  if ((kind === "tool-call" || kind === "tool-result") && subagentLabel) {
    return `${subagentExecutionActionLabel(t)} · ${subagentLabel}`;
  }
  if (kind === "tool-call") return toolName ? `${t ? t("conversation.block.toolCall") : "Tool call"} · ${toolName}` : t ? t("conversation.block.toolCall") : "Tool call";
  if (kind === "tool-result") return toolName ? `${t ? t("conversation.block.toolResult") : "Tool result"} · ${toolName}` : t ? t("conversation.block.toolResult") : "Tool result";
  if (kind === "command") return t ? t("conversation.block.command") : "Command session";
  if (kind === "diff") return "Git Diff";
  if (kind === "thinking") return t ? t("conversation.block.thinking") : "Thinking process";
  if (kind === "turn-stats") return "Turn Stats";
  if (kind === "notice") return t ? t("conversation.block.notice") : "Run notice";
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

// firstMetadataString reads the first non-empty string from all blocks by dotted paths.
// firstMetadataString 按点分路径从所有块读取第一条非空字符串。
function firstMetadataString(blocks: ToolExecutionConversationBlock[], paths: string[]): string | null {
  for (const block of blocks) {
    const value = metadataString(block.rawMetadata, paths);
    if (value) {
      return value;
    }
  }
  return null;
}

// firstPayloadObject returns the first host-result payload object carried by the merged row.
// firstPayloadObject 返回合并行携带的第一份宿主结果载荷对象。
function firstPayloadObject(blocks: ToolExecutionConversationBlock[]): Record<string, unknown> | null {
  for (const block of blocks) {
    const payload = metadataValue(block.rawMetadata, ["host_result.payload", "hostResult.payload", "payload"]);
    if (payload && typeof payload === "object" && !Array.isArray(payload)) {
      return payload as Record<string, unknown>;
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

// stringMetadata normalizes one unknown JSON value into a non-empty string.
// stringMetadata 将未知 JSON 值规整为非空字符串。
function stringMetadata(value: unknown): string | null {
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

// arrayMetadata reads one array property from a payload object using snake or camel case.
// arrayMetadata 使用蛇形或驼峰键从载荷对象读取数组属性。
function arrayMetadata(metadata: Record<string, unknown> | null, key: string): unknown[] {
  if (!metadata) {
    return [];
  }
  const camelKey = key.replace(/_([a-z])/gu, (_, letter: string) => letter.toUpperCase());
  const value = metadata[key] ?? metadata[camelKey];
  return Array.isArray(value) ? value : [];
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

// numberMetadata reads a finite number from unknown JSON values.
// numberMetadata 从未知 JSON 值读取有限数字。
function numberMetadata(value: unknown): number | null {
  const parsed = numericMetadata(value);
  return parsed === null ? null : Math.trunc(parsed);
}

// booleanMetadata reads a boolean from unknown JSON values.
// booleanMetadata 从未知 JSON 值读取布尔值。
function booleanMetadata(value: unknown): boolean | null {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "string") {
    if (value === "true") return true;
    if (value === "false") return false;
  }
  return null;
}

// booleanText localizes compact true/false values for detail sections.
// booleanText 为详情区域本地化紧凑真假值。
function booleanText(value: boolean, t?: (key: TranslationKey) => string): string {
  return value
    ? text(t, "conversation.subagent.detail.yes", "Yes")
    : text(t, "conversation.subagent.detail.no", "No");
}

// detailLine formats one key-value pair for Markdown detail panels.
// detailLine 为 Markdown 详情面板格式化一条键值对。
function detailLine(label: string, value: string): string {
  return value ? `**${label}：** ${value}` : `**${label}：**`;
}

// fencedText keeps user prompts and subagent results readable without treating them as JSON.
// fencedText 保持用户提示词与子代理返回可读，并避免被当成 JSON 展示。
function fencedText(value: string): string {
  return `\`\`\`text\n${value.trim()}\n\`\`\``;
}

// text resolves an optional i18n key with an English fallback.
// text 解析可选国际化键并提供英文兜底。
function text(t: ((key: TranslationKey) => string) | undefined, key: TranslationKey, fallback: string): string {
  return t ? t(key) : fallback;
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

