import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ConversationDetailRef } from "@/components/conversation/conversation-detail";
import type { TranslationKey } from "@/i18n-messages";
import type { ToolExecutionConversationBlock } from "./conversation-tool-execution-types";
import {
  isSubagentToolName,
  subagentToolDisplayName,
} from "./conversation-tool-metadata";

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
  if (commandBlock && !toolName) {
    return commandBlock.summary.toLowerCase().includes("command")
      ? commandBlock.summary
      : `${t ? t("conversation.block.command") : "Command session"} · ${statusLabel}`;
  }

  if (subagentLabel) {
    return `${subagentExecutionActionLabel(displayState, t)} · ${subagentLabel} · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }

  if (toolName) {
    return `${t ? t("conversation.block.toolCall") : "Tool call"} · ${toolName} · ${statusLabel}${errorPreview ? ` · ${errorPreview}` : ""}`;
  }
  return `${t ? t("conversation.block.toolCall") : "Tool execution"} · ${statusLabel}${identifier ? ` · ${identifier}` : ""}${errorPreview ? ` · ${errorPreview}` : ""}`;
}

// buildSubagentToolExecutionDetailBody builds a human-readable detail body for built-in subagent orchestration tools.
// buildSubagentToolExecutionDetailBody 为内置子代理调度工具构建面向人的详情正文。
function buildSubagentToolExecutionDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string | null {
  const toolName = toolExecutionName(blocks);
  const hostKind = firstMetadataString(blocks, ["host_result.kind", "hostResult.kind", "kind"]);
  if (!isSubagentToolName(toolName) && hostKind !== "task_session" && hostKind !== "subagent_status" && hostKind !== "subagent_message" && hostKind !== "subagent_control") {
    return null;
  }
  if (toolName === "wait_agent" || toolName === "list_agents" || hostKind === "subagent_status") {
    return buildSubagentCollectionDetailBody(blocks, t);
  }
  if (toolName === "send_message" || toolName === "followup_task" || hostKind === "subagent_message") {
    return buildSubagentMessageDetailBody(blocks, t);
  }
  if (toolName === "close_agent" || hostKind === "subagent_control") {
    return buildSubagentControlDetailBody(blocks, t);
  }
  return buildSubagentSpawnDetailBody(blocks, t);
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
    `## ${text(t, "conversation.subagent.detail.createdTask", "Created subagent task")}`,
    detailLine(text(t, "conversation.subagent.detail.name", "Name"), name),
    description ? detailLine(text(t, "conversation.subagent.detail.description", "Description"), description) : null,
    `### ${text(t, "conversation.subagent.detail.prompt", "Prompt")}`,
    prompt ? fencedText(prompt) : text(t, "conversation.subagent.detail.empty", "No content."),
  ].filter(Boolean).join("\n\n");
}

// buildSubagentCollectionDetailBody renders wait/list results as a readable agent-result list.
// buildSubagentCollectionDetailBody 将等待或列出结果渲染为可读的代理结果列表。
function buildSubagentCollectionDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const agents = arrayMetadata(payload, "agents").map(objectMetadata).filter((agent): agent is Record<string, unknown> => Boolean(agent));
  const completed = booleanMetadata(payload?.completed);
  const timeoutMs = numberMetadata(payload?.timeout_ms ?? payload?.timeoutMs);
  const header = [
    `## ${text(t, "conversation.subagent.detail.collection", "Subagent collection")}`,
    detailLine(text(t, "conversation.subagent.detail.count", "Count"), String(agents.length)),
    completed !== null ? detailLine(text(t, "conversation.subagent.detail.completed", "Completed"), booleanText(completed, t)) : null,
    timeoutMs !== null && timeoutMs > 0 ? detailLine(text(t, "conversation.subagent.detail.timeout", "Timeout"), `${timeoutMs} ms`) : null,
  ].filter(Boolean);
  if (!agents.length) {
    return [...header, text(t, "conversation.subagent.detail.noAgents", "No subagent result was returned.")].join("\n\n");
  }
  const agentSections = agents.map((agent, index) => {
    const name = stringMetadata(agent.display_title ?? agent.displayTitle ?? agent.task_name ?? agent.taskName ?? agent.agent_type ?? agent.agentType)
      ?? `${text(t, "conversation.subagent.detail.agent", "Agent")} ${index + 1}`;
    const status = stringMetadata(agent.status) ?? text(t, "conversation.subagent.detail.unknown", "Unknown");
    const result = stringMetadata(agent.result) ?? stringMetadata(agent.error) ?? "";
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

// buildSubagentMessageDetailBody renders queue and follow-up dispatch records without raw JSON.
// buildSubagentMessageDetailBody 以非 JSON 形式渲染排队与跟进调度记录。
function buildSubagentMessageDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const target = firstMetadataString(blocks, ["arguments.target", "host_result.payload.target", "hostResult.payload.target", "payload.target"])
    ?? text(t, "conversation.subagent.detail.unknown", "Unknown");
  const message = firstMetadataString(blocks, ["arguments.message"]) ?? "";
  const executionPrompt = firstMetadataString(blocks, [
    "host_result.payload.prompt",
    "hostResult.payload.prompt",
    "payload.prompt",
  ]);
  const result = firstMetadataString(blocks, [
    "host_result.payload.result",
    "hostResult.payload.result",
    "payload.result",
  ]);
  const queued = booleanMetadata(payload?.queued);
  const triggerTurn = booleanMetadata(payload?.trigger_turn ?? payload?.triggerTurn);
  const note = stringMetadata(payload?.note);
  const targetIds = arrayMetadata(payload, "target_task_session_ids").map(stringMetadata).filter((value): value is string => Boolean(value));
  return [
    `## ${text(t, "conversation.subagent.detail.messageDispatch", "Subagent message dispatch")}`,
    detailLine(text(t, "conversation.subagent.detail.target", "Target"), target),
    queued !== null ? detailLine(text(t, "conversation.subagent.detail.queued", "Queued"), booleanText(queued, t)) : null,
    triggerTurn !== null ? detailLine(text(t, "conversation.subagent.detail.triggerTurn", "Trigger turn"), booleanText(triggerTurn, t)) : null,
    targetIds.length ? detailLine(text(t, "conversation.subagent.detail.targetSessions", "Target sessions"), targetIds.join(", ")) : null,
    note ? detailLine(text(t, "conversation.subagent.detail.note", "Note"), note) : null,
    `### ${text(t, "conversation.subagent.detail.prompt", "Prompt")}`,
    message ? fencedText(message) : text(t, "conversation.subagent.detail.empty", "No content."),
    executionPrompt && executionPrompt !== message ? `### ${text(t, "conversation.subagent.detail.effectivePrompt", "Effective prompt")}\n\n${fencedText(executionPrompt)}` : null,
    result ? `### ${text(t, "conversation.subagent.detail.result", "Result")}\n\n${fencedText(result)}` : null,
  ].filter(Boolean).join("\n\n");
}

// buildSubagentControlDetailBody renders close-agent results as a compact operation summary.
// buildSubagentControlDetailBody 将关闭子代理结果渲染为紧凑操作摘要。
function buildSubagentControlDetailBody(blocks: ToolExecutionConversationBlock[], t?: (key: TranslationKey) => string): string {
  const payload = firstPayloadObject(blocks);
  const target = firstMetadataString(blocks, ["arguments.task_name", "arguments.task_id", "arguments.target"]);
  const stoppedIds = arrayMetadata(payload, "stopped_task_session_ids").map(stringMetadata).filter((value): value is string => Boolean(value));
  const stoppedCount = numberMetadata(payload?.stopped_count ?? payload?.stoppedCount) ?? stoppedIds.length;
  return [
    `## ${text(t, "conversation.subagent.detail.control", "Subagent control")}`,
    target ? detailLine(text(t, "conversation.subagent.detail.target", "Target"), target) : null,
    detailLine(text(t, "conversation.subagent.detail.stoppedCount", "Stopped count"), String(stoppedCount)),
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
  return blocks.some((block) => isSubagentToolName(block.toolName) || Boolean(block.taskSessionId))
    ? (t ? t("conversation.subagent.schedule") : "Subagent schedule")
    : null;
}

// subagentExecutionActionLabel returns a state-aware label for built-in subagent tool rows.
// subagentExecutionActionLabel 为内置子代理工具行返回带状态语义的标签。
function subagentExecutionActionLabel(status: string, t?: (key: TranslationKey) => string): string {
  return status === "running"
    ? (t ? t("conversation.subagent.calling") : "Calling subagent")
    : (t ? t("conversation.subagent.call") : "Subagent call");
}

// toolExecutionIdentifier resolves the most useful fallback identifier for a tool row.
// toolExecutionIdentifier 为工具行解析最有用的兜底标识。
export function toolExecutionIdentifier(blocks: ToolExecutionConversationBlock[]): string | null {
  for (const block of blocks) {
    const identifier = block.toolCallId
      ?? block.toolResultId
      ?? block.commandSessionId
      ?? block.taskSessionId;
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
function toolBlockTitle(block: ToolExecutionConversationBlock, t?: (key: TranslationKey) => string): string {
  const kind = block.kind;
  const toolName = block.toolName;
  const subagentLabel = toolExecutionSubagentLabel([block], t);
  if ((kind === "tool-call" || kind === "tool-result") && subagentLabel) {
    const displayState = normalizeToolDisplayState(block.status) ?? "completed";
    return `${subagentExecutionActionLabel(displayState, t)} · ${subagentLabel}`;
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
