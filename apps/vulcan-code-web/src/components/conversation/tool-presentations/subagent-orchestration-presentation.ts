import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type {
  SubagentOrchestrationAgent,
  SubagentOrchestrationPresentation,
  ToolPresentationDisplayState,
} from "./types";
import {
  argumentsFromMetadata,
  arrayValue,
  compactHash,
  fixedHostKind,
  fixedHostPayload,
  fixedToolName,
  metadataString,
  normalizeToolName,
  objectValue,
  numberValue,
  stringValue,
  textValue,
} from "./presentation-value-readers";

// resolveSubagentOrchestrationPresentation recognizes built-in subagent orchestration tool rows.
// resolveSubagentOrchestrationPresentation 识别内置子代理调度工具行。
export function resolveSubagentOrchestrationPresentation(
  // Blocks merged into one tool presentation row.
  // 合并为单个工具展示行的块。
  blocks: ToolExecutionConversationBlock[],
  // Normalized display state from the tool execution row.
  // 工具执行行传入的规范展示状态。
  displayState: ToolPresentationDisplayState,
  // Hidden detail reference for completed tool output hydration.
  // 已完成工具输出补水使用的隐藏详情引用。
  detailRef?: ConversationDetailRef,
): SubagentOrchestrationPresentation | null {
  const toolName = fixedToolName(blocks);
  const hostKind = fixedHostKind(blocks);
  const operation = subagentOperation(toolName, hostKind, blocks.some((block) => Boolean(block.taskSessionId)));
  if (!operation) {
    return null;
  }
  const args = firstArgumentsObject(blocks);
  const payload = subagentOperationPayload(operation, blocks);
  const title = subagentPresentationTitle(operation, args, payload, toolName);
  return {
    agents: operation === "collect" ? subagentAgents(payload, blocks) : [],
    detailRef,
    effectivePrompt: stringValue(payload?.prompt),
    id: subagentPresentationId(blocks, operation, title),
    kind: "subagent_orchestration",
    note: stringValue(payload?.note),
    operation,
    prompt: subagentPrompt(operation, args, payload),
    queued: booleanValue(payload?.queued),
    result: operation === "create" ? null : stringValue(payload?.result),
    state: displayState,
    stoppedCount: operation === "control" ? stoppedTaskCount(payload) : 0,
    stoppedTaskIds: operation === "control" ? stoppedTaskIds(payload) : [],
    target: subagentTarget(args, payload),
    title,
    triggerTurn: booleanValue(payload?.trigger_turn ?? payload?.triggerTurn),
  };
}

// hydrateSubagentOrchestrationPresentation merges hidden full tool output into one subagent presentation.
// hydrateSubagentOrchestrationPresentation 将隐藏完整工具输出合并进子代理展示模型。
export function hydrateSubagentOrchestrationPresentation(
  // Existing presentation model.
  // 现有展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Hidden tool detail payload.
  // 隐藏工具详情载荷。
  payload: ToolDetailPayload | null,
): SubagentOrchestrationPresentation {
  if (!payload) {
    return presentation;
  }
  const parsed = parseJsonObject(payload.body);
  if (!parsed || !subagentPayloadMatchesOperation(presentation.operation, parsed)) {
    return presentation;
  }
  return mergeSubagentPayload(presentation, parsed);
}

// mergeSubagentPayload overlays one verified runtime payload onto the presentation.
// mergeSubagentPayload 将一个已验证的运行时载荷覆盖到展示模型。
function mergeSubagentPayload(
  // Existing presentation model.
  // 现有展示模型。
  presentation: SubagentOrchestrationPresentation,
  // Verified runtime payload.
  // 已验证的运行时载荷。
  payload: Record<string, unknown>,
): SubagentOrchestrationPresentation {
  return {
    ...presentation,
    agents: presentation.operation === "collect" ? subagentAgents(payload, []) : presentation.agents,
    effectivePrompt: stringValue(payload.prompt) ?? presentation.effectivePrompt,
    note: stringValue(payload.note) ?? presentation.note,
    prompt: presentation.prompt ?? stringValue(payload.prompt),
    queued: booleanValue(payload.queued) ?? presentation.queued,
    result: presentation.operation === "create" ? null : stringValue(payload.result) ?? presentation.result,
    stoppedCount: presentation.operation === "control" ? stoppedTaskCount(payload) : presentation.stoppedCount,
    stoppedTaskIds: presentation.operation === "control" ? stoppedTaskIds(payload) : presentation.stoppedTaskIds,
    target: presentation.target ?? stringValue(payload.target),
    triggerTurn: booleanValue(payload.trigger_turn ?? payload.triggerTurn) ?? presentation.triggerTurn,
  };
}

// subagentOperationPayload resolves the payload that matches the current subagent operation.
// subagentOperationPayload 解析与当前子代理操作匹配的载荷。
function subagentOperationPayload(
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Blocks to inspect.
  // 需要检查的块。
  blocks: ToolExecutionConversationBlock[],
): Record<string, unknown> | null {
  const hostPayload = fixedHostPayload(blocks);
  if (hostPayload && subagentPayloadMatchesOperation(operation, hostPayload)) {
    return hostPayload;
  }
  return firstBodyJsonObject(blocks, (payload) => subagentPayloadMatchesOperation(operation, payload))
    ?? hostPayload
    ?? (operation === "create" ? firstBodyJsonObject(blocks) : null);
}

// subagentPayloadMatchesOperation verifies one payload against the known runtime tool contract.
// subagentPayloadMatchesOperation 根据已知运行时工具契约校验单个载荷。
function subagentPayloadMatchesOperation(
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Candidate payload object.
  // 候选载荷对象。
  payload: Record<string, unknown>,
): boolean {
  if (operation === "collect") {
    return Array.isArray(payload.agents);
  }
  if (operation === "control") {
    return "stopped_count" in payload
      || "stoppedCount" in payload
      || "stopped_task_session_ids" in payload
      || "stoppedTaskSessionIds" in payload;
  }
  if (operation === "message" || operation === "followup") {
    return "target" in payload
      || "target_task_session_ids" in payload
      || "targetTaskSessionIds" in payload
      || "queued" in payload
      || "trigger_turn" in payload
      || "triggerTurn" in payload;
  }
  return "task_session_id" in payload
    || "taskSessionId" in payload
    || "display_title" in payload
    || "displayTitle" in payload
    || "prompt" in payload;
}

// subagentOperation maps tool names and host-result kinds onto display operations.
// subagentOperation 将工具名与宿主结果类型映射为展示操作。
function subagentOperation(
  // Tool name from the visible row.
  // 可见行中的工具名。
  toolName: string | null,
  // Host result kind from metadata.
  // 元数据中的宿主结果类型。
  hostKind: string | null,
  // Whether the normalized transcript row carries a child task-session id.
  // 规范化转录行是否携带子任务会话标识。
  hasTaskSessionId: boolean,
): SubagentOrchestrationPresentation["operation"] | null {
  const normalized = toolName ? normalizeToolName(toolName) : null;
  if (normalized === "spawn_agent") return "create";
  if (normalized === "wait_agent" || normalized === "list_agents" || hostKind === "subagent_status") return "collect";
  if (normalized === "send_message" || hostKind === "subagent_message") return "message";
  if (normalized === "followup_task") return "followup";
  if (normalized === "close_agent" || hostKind === "subagent_control") return "control";
  if (hostKind === "task_session" || hasTaskSessionId) return "create";
  return null;
}

// firstArgumentsObject returns the first plain tool-arguments object.
// firstArgumentsObject 返回第一份普通工具参数对象。
function firstArgumentsObject(
  // Blocks to inspect.
  // 需要检查的块。
  blocks: ToolExecutionConversationBlock[],
): Record<string, unknown> | null {
  return blocks
    .map((block) => objectValue(argumentsFromMetadata(block.rawMetadata)))
    .find(Boolean)
    ?? null;
}

// firstBodyJsonObject parses the first visible JSON object body or summary.
// firstBodyJsonObject 解析第一段可见 JSON 对象正文或摘要。
function firstBodyJsonObject(
  // Blocks to inspect.
  // 需要检查的块。
  blocks: ToolExecutionConversationBlock[],
  // Optional predicate that must accept the parsed object.
  // 可选的解析对象验收条件。
  predicate?: (payload: Record<string, unknown>) => boolean,
): Record<string, unknown> | null {
  for (const block of resultFirstBlocks(blocks)) {
    const parsed = parseJsonObject(block.body) ?? parseJsonObject(block.summary);
    if (parsed && (!predicate || predicate(parsed))) {
      return parsed;
    }
  }
  return null;
}

// resultFirstBlocks orders tool-result blocks before tool-call argument blocks.
// resultFirstBlocks 将工具结果块排在工具调用参数块之前。
function resultFirstBlocks(
  // Blocks to order.
  // 需要排序的块。
  blocks: ToolExecutionConversationBlock[],
): ToolExecutionConversationBlock[] {
  return [
    ...blocks.filter((block) => block.kind === "tool-result"),
    ...blocks.filter((block) => block.kind !== "tool-result"),
  ];
}

// parseJsonObject extracts one JSON object from plain or fenced text.
// parseJsonObject 从普通文本或代码块文本中提取 JSON 对象。
function parseJsonObject(
  // Source text.
  // 源文本。
  source: string | null | undefined,
): Record<string, unknown> | null {
  const trimmed = source?.trim();
  if (!trimmed) {
    return null;
  }
  const firstBrace = trimmed.indexOf("{");
  const lastBrace = trimmed.lastIndexOf("}");
  if (firstBrace < 0 || lastBrace <= firstBrace) {
    return null;
  }
  try {
    return objectValue(JSON.parse(trimmed.slice(firstBrace, lastBrace + 1)));
  } catch {
    return null;
  }
}

// subagentPresentationTitle builds the compact row title.
// subagentPresentationTitle 构建紧凑行标题。
function subagentPresentationTitle(
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Tool arguments.
  // 工具参数。
  args: Record<string, unknown> | null,
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
  // Original tool name.
  // 原始工具名。
  toolName: string | null,
): string {
  if (operation === "collect") {
    return stringValue(payload?.tool) ?? toolName ?? "subagent";
  }
  return subagentTarget(args, payload)
    ?? stringValue(payload?.display_title ?? payload?.displayTitle)
    ?? stringValue(payload?.task_id ?? payload?.taskId)
    ?? toolName
    ?? "subagent";
}

// subagentPrompt resolves the one-call prompt/message text.
// subagentPrompt 解析本次调用的提示词或消息文本。
function subagentPrompt(
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Tool arguments.
  // 工具参数。
  args: Record<string, unknown> | null,
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): string | null {
  if (operation === "create") {
    return textValue(args?.message) ?? textValue(args?.prompt) ?? textValue(payload?.prompt);
  }
  if (operation === "message" || operation === "followup") {
    return textValue(args?.message) ?? textValue(payload?.message);
  }
  return null;
}

// subagentTarget resolves the target task name for a dispatch row.
// subagentTarget 解析调度行的目标任务名。
function subagentTarget(
  // Tool arguments.
  // 工具参数。
  args: Record<string, unknown> | null,
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): string | null {
  return stringValue(args?.target)
    ?? stringValue(args?.task_name ?? args?.taskName)
    ?? stringValue(args?.task_id ?? args?.taskId)
    ?? stringValue(payload?.target)
    ?? stringValue(payload?.task_id ?? payload?.taskId);
}

// subagentAgents converts status payload agents into compact rows.
// subagentAgents 将状态载荷中的 agents 转换为紧凑行。
function subagentAgents(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
  // Blocks used as fallback source.
  // 用作兜底来源的块。
  blocks: ToolExecutionConversationBlock[],
): SubagentOrchestrationAgent[] {
  const rawAgents = arrayValue(payload?.agents);
  return rawAgents.map((rawAgent, index) => {
    const agent = objectValue(rawAgent);
    const name = stringValue(agent?.display_title ?? agent?.displayTitle)
      ?? stringValue(agent?.task_name ?? agent?.taskName)
      ?? stringValue(agent?.agent_type ?? agent?.agentType)
      ?? `agent-${index + 1}`;
    return {
      id: stringValue(agent?.task_session_id ?? agent?.taskSessionId)
        ?? `${index}:${compactHash(JSON.stringify(rawAgent))}`,
      name,
      result: stringValue(agent?.result) ?? fallbackResultFromBlocks(blocks),
      status: stringValue(agent?.status) ?? "unknown",
    };
  });
}

// fallbackResultFromBlocks returns a result text only when a row carries plain non-JSON output.
// fallbackResultFromBlocks 仅在行携带普通非 JSON 输出时返回结果文本。
function fallbackResultFromBlocks(
  // Blocks to inspect.
  // 需要检查的块。
  blocks: ToolExecutionConversationBlock[],
): string | null {
  for (const block of blocks) {
    const body = block.body.trim();
    if (body && !parseJsonObject(body)) {
      return body;
    }
  }
  return null;
}

// stoppedTaskIds reads stopped task ids from control payloads.
// stoppedTaskIds 从控制载荷读取已停止任务标识。
function stoppedTaskIds(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): string[] {
  return arrayValue(payload?.stopped_task_session_ids ?? payload?.stoppedTaskSessionIds)
    .map(stringValue)
    .filter((value): value is string => Boolean(value));
}

// stoppedTaskCount reads the explicit stopped count or falls back to the id list length.
// stoppedTaskCount 读取显式停止数量，缺失时回退为标识列表长度。
function stoppedTaskCount(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): number {
  return numberValue(payload?.stopped_count ?? payload?.stoppedCount) ?? stoppedTaskIds(payload).length;
}

// booleanValue normalizes boolean or string boolean values.
// booleanValue 规整布尔值或字符串布尔值。
function booleanValue(
  // Source value.
  // 源值。
  value: unknown,
): boolean | null {
  if (typeof value === "boolean") {
    return value;
  }
  if (value === "true") return true;
  if (value === "false") return false;
  return null;
}

// subagentPresentationId builds a stable expansion id.
// subagentPresentationId 构建稳定展开标识。
function subagentPresentationId(
  // Blocks used to derive identifiers.
  // 用于派生标识的块。
  blocks: ToolExecutionConversationBlock[],
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Compact title.
  // 紧凑标题。
  title: string,
): string {
  const rawId = blocks.map((block) => block.toolCallId ?? block.toolResultId ?? block.id).join(":");
  return `subagent:${operation}:${compactHash(`${rawId}:${title}`)}`;
}
