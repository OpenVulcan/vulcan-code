import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type {
  CollaborationToolArguments,
  SubagentOrchestrationAgent,
  SubagentOrchestrationPresentation,
  SubagentUsageSummary,
  ToolPresentationDisplayState,
} from "./types";
import {
  argumentsFromMetadata,
  arrayValue,
  compactHash,
  fixedHostKind,
  fixedHostPayload,
  fixedToolName,
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
  const payloadHint = fixedHostPayload(blocks) ?? firstBodyJsonObject(blocks);
  const operation = subagentOperation(toolName, hostKind, blocks.some((block) => Boolean(block.subagentTaskId)), payloadHint);
  if (!operation) {
    return null;
  }
  const args = firstArgumentsObject(blocks);
  const collaborationArguments = collaborationToolArguments(toolName, args);
  const payload = isCollaborationOperation(operation)
    ? collaborationOutputPayload(blocks)
    : subagentOperationPayload(operation, blocks);
  const title = subagentPresentationTitle(operation, args, payload, toolName);
  const operationHasAgents = operation === "agent_orchestration_info" || operation === "list_agents" || operation === "wait";
  const operationClosesAgents = operation === "close";
  const operationStopsAgents = operation === "close" || operation === "interrupt";
  return {
    // ask_question returns the new ID; reply/wait/cancel receive an existing ID in their input.
    // ask_question 返回新身份；回复、等待及撤销工具通过输入接收已有身份。
    questionId: operation === "collaboration_question" ? stringValue(payload?.question_id)
      : operation === "collaboration_answer" || operation === "collaboration_question_wait" || operation === "collaboration_cancel"
        ? stringValue(args?.question_id) : null,
    executionId: operation === "create" ? stringValue(payload?.execution_id) : null,
    answerText: isCollaborationOperation(operation) ? stringValue(objectValue(payload?.answer)?.body) : null,
    agents: operationHasAgents ? subagentAgents(payload, blocks) : [],
    agentsKnown: operationHasAgents ? subagentAgentsKnown(payload) : true,
    debugSummary: subagentDebugSummary(payload?.debug),
    detailRef,
    effectivePrompt: isCollaborationOperation(operation) ? null : stringValue(payload?.prompt),
    id: subagentPresentationId(blocks, operation, title),
    kind: "subagent_orchestration",
    note: isCollaborationOperation(operation)
      ? null
      : stringValue(payload?.note) ?? (operation === "wait" || operation === "agent_orchestration_info" || operation === "list_agents" ? stringValue(payload?.message) : null),
    operation,
    prompt: subagentPrompt(operation, args, payload),
    queued: isCollaborationOperation(operation)
      ? collaborationDeliveryMode(collaborationArguments) === "queue"
        ? true
        : null
      : booleanValue(payload?.queued),
    deliveryMode: isCollaborationOperation(operation)
      ? collaborationDeliveryMode(collaborationArguments)
      : stringValue(payload?.delivery_mode ?? payload?.deliveryMode),
    result: subagentResult(operation, payload, blocks),
    state: displayState,
    closedAgents: operationClosesAgents ? closedAgents(payload) : [],
    closedCount: operationClosesAgents ? closedAgentCount(payload) : 0,
    clearedMessageCount: operationClosesAgents ? clearedMessageCount(payload) : null,
    stoppedCount: operationStopsAgents ? stoppedTaskCount(payload) : 0,
    stoppedTaskIds: operationStopsAgents ? stoppedTaskIds(payload) : [],
    target: subagentTarget(args, payload),
    timedOut: operation === "wait" ? waitTimedOut(payload) : null,
    waitNeedsReply: operation === "wait" && isUnifiedWaitPayload(payload) && payload?.status === "needs_reply",
    waitCheckFailed: operation === "wait" && isUnifiedWaitPayload(payload) && payload?.status === "check_failed",
    noActiveAgents: operation === "wait" ? subagentWaitHasNoActiveAgents(payload) : false,
    interrupted: operation === "wait" ? booleanValue(payload?.interrupted) : null,
    terminalEventReceived: operation === "wait" ? subagentWaitTerminalEventReceived(payload) : false,
    collaborationArguments,
    title,
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
  const operationHasAgents = presentation.operation === "agent_orchestration_info" || presentation.operation === "list_agents" || presentation.operation === "wait";
  const agents = operationHasAgents
    ? mergeSubagentAgents(presentation.agents, subagentAgents(payload, []))
    : presentation.agents;
  const operationClosesAgents = presentation.operation === "close";
  const operationStopsAgents = presentation.operation === "close" || presentation.operation === "interrupt";
  return {
    ...presentation,
    // Hydrated collaboration receipts carry the same typed answer body as the initial payload.
    // 补载协作回执携带与初始载荷相同的强类型回答正文。
    answerText: isCollaborationOperation(presentation.operation) ? stringValue(objectValue(payload.answer)?.body) : presentation.answerText,
    agents,
    agentsKnown: operationHasAgents ? subagentAgentsKnown(payload) : presentation.agentsKnown,
    debugSummary: isCollaborationOperation(presentation.operation)
      ? presentation.debugSummary
      : subagentDebugSummary(payload.debug) ?? presentation.debugSummary,
    effectivePrompt: isCollaborationOperation(presentation.operation)
      ? presentation.effectivePrompt
      : stringValue(payload.prompt) ?? presentation.effectivePrompt,
    note: isCollaborationOperation(presentation.operation)
      ? presentation.note
      : stringValue(payload.note) ?? presentation.note,
    prompt: isCollaborationOperation(presentation.operation)
      ? presentation.prompt
      : presentation.prompt ?? stringValue(payload.prompt),
    queued: isCollaborationOperation(presentation.operation)
      ? presentation.queued
      : booleanValue(payload.queued) ?? presentation.queued,
    deliveryMode: isCollaborationOperation(presentation.operation)
      ? presentation.deliveryMode
      : stringValue(payload.delivery_mode ?? payload.deliveryMode) ?? presentation.deliveryMode,
    result: subagentResult(presentation.operation, payload) ?? presentation.result,
    closedAgents: operationClosesAgents ? closedAgents(payload) : presentation.closedAgents,
    closedCount: operationClosesAgents ? closedAgentCount(payload) : presentation.closedCount,
    clearedMessageCount: operationClosesAgents ? clearedMessageCount(payload) : presentation.clearedMessageCount,
    stoppedCount: operationStopsAgents ? stoppedTaskCount(payload) : presentation.stoppedCount,
    stoppedTaskIds: operationStopsAgents ? stoppedTaskIds(payload) : presentation.stoppedTaskIds,
    target: isCollaborationOperation(presentation.operation)
      ? presentation.target
      : presentation.target ?? stringValue(payload.target),
    timedOut: presentation.operation === "wait"
      ? waitTimedOut(payload) ?? presentation.timedOut
      : presentation.timedOut,
    waitNeedsReply: presentation.operation === "wait" && isUnifiedWaitPayload(payload)
      ? payload.status === "needs_reply" : presentation.waitNeedsReply,
    waitCheckFailed: presentation.operation === "wait" && isUnifiedWaitPayload(payload)
      ? payload.status === "check_failed" : presentation.waitCheckFailed,
    noActiveAgents: presentation.operation === "wait"
      ? subagentWaitHasNoActiveAgents(payload) || presentation.noActiveAgents
      : presentation.noActiveAgents,
    interrupted: presentation.operation === "wait"
      ? booleanValue(payload.interrupted) ?? presentation.interrupted
      : presentation.interrupted,
    terminalEventReceived: presentation.operation === "wait"
      ? subagentWaitTerminalEventReceived(payload) || presentation.terminalEventReceived
      : presentation.terminalEventReceived,
  };
}

// mergeSubagentAgents keeps visible usage metadata when hydrated detail omits host-only usage.
// mergeSubagentAgents 在详情补水缺少宿主专用消耗时保留可见记录中的消耗信息。
function mergeSubagentAgents(
  // Existing visible agent rows.
  // 现有可见智能体行。
  existingAgents: SubagentOrchestrationAgent[],
  // Newly hydrated agent rows.
  // 新补水得到的智能体行。
  incomingAgents: SubagentOrchestrationAgent[],
): SubagentOrchestrationAgent[] {
  if (incomingAgents.length === 0) {
    return existingAgents;
  }
  return incomingAgents.map((agent) => {
    if (agent.usageSummary) {
      return agent;
    }
    const existing = existingAgents.find((candidate) => candidate.id === agent.id)
      ?? existingAgents.find((candidate) => candidate.name === agent.name);
    return existing?.usageSummary
      ? { ...agent, usageSummary: existing.usageSummary }
      : agent;
  });
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
  if (isCollaborationOperation(operation)) {
    return true;
  }
  if (operation === "wait") {
    return isUnifiedWaitPayload(payload) || "message" in payload || "timed_out" in payload || "timedOut" in payload || "interrupted" in payload;
  }
  if (operation === "agent_orchestration_info" || operation === "list_agents") {
    return Array.isArray(payload.agents);
  }
  if (operation === "close") {
    return "closed" in payload
      || "closed_count" in payload
      || "closedCount" in payload
      || "closed_agents" in payload
      || "closedAgents" in payload
      || "cleared_message_count" in payload
      || "clearedMessageCount" in payload;
  }
  if (operation === "interrupt") {
    return "stopped_count" in payload
      || "stoppedCount" in payload
      || "stopped_subagent_task_ids" in payload
      || "stoppedSubagentTaskIds" in payload;
  }
  if (operation === "message") {
    return "message" in payload
      || "target" in payload
      || "delivery_mode" in payload
      || "deliveryMode" in payload;
  }
  if (operation === "feedback") {
    return "result" in payload
      || "message" in payload
      || "subagent" in payload
      || "run_state" in payload
      || "runState" in payload;
  }
  return "subagent_task_id" in payload
    || "subagentTaskId" in payload
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
  // Whether the normalized transcript row carries a child subagent task id.
  // 规范化转录行是否携带子代理任务标识。
  hasSubagentTaskId: boolean,
  // Optional payload hint used to classify legacy subagent control rows.
  // 用于分类旧式子代理控制行的可选载荷线索。
  payloadHint: Record<string, unknown> | null,
): SubagentOrchestrationPresentation["operation"] | null {
  const normalized = toolName ? normalizeToolName(toolName) : null;
  if (normalized === "wait_agents_result") return "wait";
  if (normalized === "ask_question" || normalized === "ask_parent") return "collaboration_question";
  if (normalized === "reply_question") return "collaboration_answer";
  if (normalized === "send_session_message") return "collaboration_steer";
  if (normalized === "send_notification") return "collaboration_notification";
  if (normalized === "wait_question") return "collaboration_question_wait";
  if (normalized === "list_sessions") return "collaboration_session_list";
  if (normalized === "read_session") return "collaboration_session_read";
  if (normalized === "wait_session") return "collaboration_session_wait";
  if (normalized === "cancel_question") return "collaboration_cancel";
  if (normalized === "spawn_agent") return "create";
  if (normalized === "send_message" || hostKind === "subagent_message") return "message";
  if (normalized === "wait_agent" || hostKind === "subagent_wait") return "wait";
  if (normalized === "agent_orchestration_info") return "agent_orchestration_info";
  if (normalized === "list_agents" || hostKind === "subagent_status") return "list_agents";
  if (normalized === "subagent_feedback" || hostKind === "subagent_feedback") return "feedback";
  if (normalized === "close_agent") return "close";
  if (normalized === "interrupt_agent") return "interrupt";
  if (hostKind === "subagent_control") return subagentControlOperation(payloadHint);
  if (hostKind === "subagent_task" || hasSubagentTaskId) return "create";
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
  if (operation === "collaboration_question") return "Agent collaboration question";
  if (operation === "collaboration_answer") return "Question answer receipt";
  if (operation === "collaboration_steer") return "Agent task direction";
  if (operation === "collaboration_question_wait") return "Wait for question settlement";
  if (operation === "collaboration_notification") return "Agent phase notification";
  if (operation === "collaboration_session_list") return "Authorized session list";
  if (operation === "collaboration_session_read") return "Authorized session read";
  if (operation === "collaboration_session_wait") return "Wait for session revision";
  if (operation === "collaboration_cancel") return "Cancel collaboration question";
  if (operation === "wait") {
    return toolName ?? "wait_agent";
  }
  if (operation === "agent_orchestration_info" || operation === "list_agents") {
    return stringValue(payload?.tool) ?? toolName ?? "subagent";
  }
  if (operation === "feedback") {
    return subagentTarget(args, payload)
      ?? stringValue(payload?.subagent)
      ?? stringValue(payload?.task_id ?? payload?.taskId)
      ?? toolName
      ?? "subagent_feedback";
  }
  if (operation === "message") {
    return subagentTarget(args, payload) ?? toolName ?? "subagent";
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
  if (operation === "collaboration_question") {
    return textValue(args?.question);
  }
  if (operation === "collaboration_answer") {
    return textValue(args?.answer);
  }
  if (operation === "collaboration_steer") {
    return textValue(args?.message);
  }
  if (operation === "collaboration_notification") {
    return textValue(args?.message);
  }
  if (operation === "collaboration_cancel") {
    return textValue(args?.reason);
  }
  if (operation === "create") {
    return textValue(args?.message) ?? textValue(args?.prompt) ?? textValue(payload?.prompt);
  }
  if (operation === "message") {
    return textValue(args?.message) ?? textValue(payload?.message);
  }
  return null;
}

// subagentControlOperation resolves legacy control rows into concrete operations.
// subagentControlOperation 将旧式控制行解析为具体操作。
function subagentControlOperation(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): "close" | "interrupt" | null {
  const normalized = normalizeSubagentToolIdentity(stringValue(payload?.tool));
  if (normalized === "close_agent") {
    return "close";
  }
  if (normalized === "interrupt_agent") {
    return "interrupt";
  }
  if (closedAgentCount(payload) > 0 || closedAgents(payload).length > 0 || clearedMessageCount(payload) !== null) {
    return "close";
  }
  if (stoppedTaskCount(payload) > 0 || stoppedTaskIds(payload).length > 0) {
    return "interrupt";
  }
  return null;
}

// normalizeSubagentToolIdentity converts optional tool identifiers into canonical comparison form.
// normalizeSubagentToolIdentity 将可选工具标识转换为规范比较形式。
function normalizeSubagentToolIdentity(
  // Raw tool identifier.
  // 原始工具标识。
  value: string | null,
): string | null {
  return value ? normalizeToolName(value) : null;
}

// subagentResult resolves visible result text for operations that expose child output.
// subagentResult 解析会暴露智能体输出的操作结果文本。
function subagentResult(
  // Normalized operation.
  // 规范化操作。
  operation: SubagentOrchestrationPresentation["operation"],
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
  blocks: ToolExecutionConversationBlock[] = [],
): string | null {
  if (isCollaborationOperation(operation)) {
    return payload ? JSON.stringify(payload) : fallbackResultFromBlocks(blocks);
  }
  if (operation === "create") {
    return null;
  }
  if (operation === "feedback") {
    return stringValue(payload?.result) ?? stringValue(payload?.message);
  }
  return stringValue(payload?.result);
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

// subagentAgentsKnown returns whether a listing payload definitively carries an agents array.
// subagentAgentsKnown 返回列表载荷是否明确携带 agents 数组。
function subagentAgentsKnown(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): boolean {
  return Array.isArray(payload?.agents);
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
      ?? stringValue(agent?.agent_name ?? agent?.agentName)
      ?? stringValue(agent?.task_name ?? agent?.taskName)
      ?? stringValue(agent?.agent_id ?? agent?.agentId)
      ?? `agent-${index + 1}`;
    return {
      id: stringValue(agent?.subagent_task_id ?? agent?.subagentTaskId)
        ?? stringValue(agent?.agent_name ?? agent?.agentName)
        ?? stringValue(agent?.agent_id ?? agent?.agentId)
        ?? `${index}:${compactHash(JSON.stringify(rawAgent))}`,
      name,
      result: subagentAgentStatusResult(agent) ?? stringValue(agent?.result) ?? stringValue(agent?.error) ?? fallbackResultFromBlocks(blocks),
      status: subagentAgentStatusText(agent) ?? stringValue(agent?.status) ?? "unknown",
      usageSummary: subagentUsageSummary(agent),
      debugSummary: subagentDebugSummary(agent?.debug),
    };
  });
}

// subagentWaitTerminalEventReceived checks the wait_agent contract for terminal child events.
// subagentWaitTerminalEventReceived 根据 wait_agent 契约检查是否收到子智能体终态事件。
function subagentWaitTerminalEventReceived(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): boolean {
  if (isUnifiedWaitPayload(payload)) return payload?.status === "messages";
  const terminalEventCount = numberValue(payload?.terminal_event_count ?? payload?.terminalEventCount);
  if (terminalEventCount !== null && terminalEventCount > 0) {
    return true;
  }
  const activityStatus = stringValue(payload?.activity_status ?? payload?.activityStatus)?.toLowerCase();
  if (activityStatus === "terminal_update") {
    return true;
  }
  return arrayValue(payload?.agents).some((rawAgent) => {
    const agent = objectValue(rawAgent);
    return booleanValue(agent?.terminal) === true || subagentAgentHasTerminalState(agent);
  });
}

// subagentWaitHasNoActiveAgents recognizes the authoritative immediate wait completion state.
// subagentWaitHasNoActiveAgents 识别权威的无活动子代理即时等待完成状态。
function subagentWaitHasNoActiveAgents(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): boolean {
  if (isUnifiedWaitPayload(payload)) return payload?.status === "no_pending_work";
  if (booleanValue(payload?.no_active_agents ?? payload?.noActiveAgents) === true) {
    return true;
  }
  return stringValue(payload?.activity_status ?? payload?.activityStatus)?.toLowerCase()
    === "no_active_agents";
}

// isUnifiedWaitPayload validates the single inbox contract without interpreting question revisions.
// isUnifiedWaitPayload 校验统一收件契约，不解读问题修订号。
function isUnifiedWaitPayload(payload: Record<string, unknown> | null): boolean {
  return payload !== null && Array.isArray(payload.reports)
    && typeof payload.context_messages_pending === "boolean"
    && (payload.status === "messages" || payload.status === "timeout" || payload.status === "no_pending_work" || payload.status === "interrupted"
      || payload.status === "needs_reply" || payload.status === "check_failed");
}

// waitTimedOut reads the declared unified result or the existing native mailbox contract.
// waitTimedOut 读取已声明统一结果或现有原生邮箱契约。
function waitTimedOut(payload: Record<string, unknown> | null): boolean | null {
  return isUnifiedWaitPayload(payload) ? payload?.status === "timeout" : booleanValue(payload?.timed_out ?? payload?.timedOut);
}

// isCollaborationOperation identifies the exact native collaboration card family.
// isCollaborationOperation 识别原生协作卡片的精确操作族。
function isCollaborationOperation(
  operation: SubagentOrchestrationPresentation["operation"],
): boolean {
  return operation === "collaboration_question"
    || operation === "collaboration_answer"
    || operation === "collaboration_steer"
    || operation === "collaboration_notification"
    || operation === "collaboration_question_wait"
    || operation === "collaboration_session_list"
    || operation === "collaboration_session_read"
    || operation === "collaboration_session_wait"
    || operation === "collaboration_cancel";
}

// collaborationOutputPayload reads only a generic result body for a collaboration tool.
// collaborationOutputPayload 只读取协作工具的通用结果正文。
function collaborationOutputPayload(
  blocks: ToolExecutionConversationBlock[],
): Record<string, unknown> | null {
  const hostPayload = fixedHostPayload(blocks);
  if (hostPayload) {
    return hostPayload;
  }
  return firstBodyJsonObject(blocks);
}

// collaborationToolArguments validates the exact native argument shape without probing result fields.
// collaborationToolArguments 校验原生精确参数形状，不探测结果字段。
function collaborationToolArguments(
  toolName: string | null,
  args: Record<string, unknown> | null,
): CollaborationToolArguments | null {
  const normalized = toolName ? normalizeToolName(toolName) : null;
  if (!normalized || !args) {
    return null;
  }
  switch (normalized) {
    case "ask_question": {
      const targetSessionId = exactString(args.target_session_id);
      const question = exactString(args.question);
      return targetSessionId && question
        ? { target_session_id: targetSessionId, question }
        : null;
    }
    case "ask_parent": {
      const question = exactString(args.question);
      return question ? { question } : null;
    }
    case "reply_question": {
      const questionId = exactString(args.question_id);
      const answer = exactString(args.answer);
      return questionId && answer
        ? { question_id: questionId, answer, unable_to_answer: args.unable_to_answer === true }
        : null;
    }
    case "wait_question": {
      const questionId = exactString(args.question_id);
      const timeoutMs = exactNumber(args.timeout_ms);
      if (!questionId || timeoutMs === null) return null;
      return {
        question_id: questionId,
        after_revision: exactOptionalNumber(args.after_revision),
        timeout_ms: timeoutMs,
      };
    }
    case "cancel_question": {
      const questionId = exactString(args.question_id);
      const reason = exactString(args.reason);
      return questionId && reason
        ? { question_id: questionId, reason }
        : null;
    }
    case "list_sessions": {
      const limit = exactNumber(args.limit);
      return limit === null
        ? null
        : { limit, cursor: exactOptionalString(args.cursor) };
    }
    case "read_session": {
      const sessionId = exactString(args.session_id);
      const limit = exactNumber(args.limit);
      return sessionId && limit !== null
        ? { session_id: sessionId, after_seq: exactOptionalNumber(args.after_seq), limit }
        : null;
    }
    case "send_session_message": {
      const sessionId = exactString(args.session_id);
      const message = exactString(args.message);
      const deliveryMode = args.delivery_mode === "queue" || args.delivery_mode === "interrupt"
        ? args.delivery_mode
        : null;
      return sessionId && message && deliveryMode
         ? { session_id: sessionId, message, delivery_mode: deliveryMode }
         : null;
    }
    case "send_notification": {
      const targetSessionId = exactString(args.target_session_id);
      const message = exactString(args.message);
      return targetSessionId && message
        ? { target_session_id: targetSessionId, message }
        : null;
    }
    case "wait_session": {
      const sessionId = exactString(args.session_id);
      const timeoutMs = exactNumber(args.timeout_ms);
      if (!sessionId || timeoutMs === null) return null;
      return {
        session_id: sessionId,
        after_revision: exactOptionalNumber(args.after_revision),
        timeout_ms: timeoutMs,
      };
    }
    default:
      return null;
  }
}

// collaborationDeliveryMode reads delivery mode only from the exact send-session-message arguments.
// collaborationDeliveryMode 只从精确的 send-session-message 参数读取投递模式。
function collaborationDeliveryMode(
  args: CollaborationToolArguments | null,
): "queue" | "interrupt" | null {
  if (!args || !("delivery_mode" in args)) return null;
  return args.delivery_mode;
}

// exactString accepts only the native string field and never searches alternate paths.
// exactString 只接受原生字符串字段，绝不搜索替代路径。
function exactString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

// exactNumber accepts only a finite native JSON number.
// exactNumber 只接受有限的原生 JSON 数值。
function exactNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

// exactOptionalNumber preserves absence without inventing a default.
// exactOptionalNumber 保留字段缺失，不发明默认值。
function exactOptionalNumber(value: unknown): number | null {
  return value === undefined || value === null ? null : exactNumber(value);
}

// exactOptionalString preserves an omitted cursor as null.
// exactOptionalString 将缺失的游标保留为 null。
function exactOptionalString(value: unknown): string | null {
  return value === undefined || value === null ? null : exactString(value);
}

// subagentAgentHasTerminalState recognizes terminal wait_agent rows from compact agent state.
// subagentAgentHasTerminalState 根据紧凑智能体状态识别 wait_agent 终态行。
function subagentAgentHasTerminalState(
  // Parsed agent payload.
  // 已解析的智能体载荷。
  agent: Record<string, unknown> | null,
): boolean {
  const status = stringValue(agent?.status ?? agent?.kind ?? agent?.task_status ?? agent?.taskStatus ?? agent?.execution_status ?? agent?.executionStatus)
    ?.toLowerCase();
  return status === "completed" || status === "failed" || status === "stopped" || status === "errored";
}

// subagentUsageSummary folds child-agent usage segments into one compact display summary.
// subagentUsageSummary 将子智能体用量分段折叠为一条紧凑展示统计。
function subagentUsageSummary(
  // Parsed agent payload.
  // 已解析的代理载荷。
  agent: Record<string, unknown> | null,
): SubagentUsageSummary | null {
  // Canonical V1 return statistics include all previously unreturned child execution segments.
  // 规范 V1 回流统计包含此前尚未回流的全部子代理执行分段。
  const statistics = objectValue(agent?.statistics);
  if (statistics) {
    const usage = objectValue(statistics.usage);
    return {
      providerCallCount: numberValue(statistics.api_call_count) ?? 0,
      toolCallCount: numberValue(statistics.tool_call_count) ?? 0,
      totalTokens: numberValue(usage?.total_tokens) ?? 0,
      cachedInputTokens: numberValue(usage?.cache_read_tokens) ?? 0,
    };
  }
  const segments = arrayValue(agent?.usage_segments ?? agent?.usageSegments);
  if (segments.length === 0) {
    return null;
  }
  const summary: SubagentUsageSummary = {
    providerCallCount: 0,
    toolCallCount: 0,
    totalTokens: 0,
    cachedInputTokens: 0,
  };
  for (const rawSegment of segments) {
    const segment = objectValue(rawSegment);
    if (!segment) {
      continue;
    }
    summary.providerCallCount += Math.max(0, numberValue(segment.provider_call_count ?? segment.providerCallCount) ?? 0);
    summary.toolCallCount += Math.max(0, numberValue(segment.tool_call_count ?? segment.toolCallCount) ?? 0);
    summary.cachedInputTokens += Math.max(0, numberValue(segment.cached_input_tokens ?? segment.cachedInputTokens) ?? 0);
    const totalTokens = numberValue(segment.total_tokens ?? segment.totalTokens);
    if (totalTokens !== null && totalTokens > 0) {
      summary.totalTokens += totalTokens;
      continue;
    }
    const inputTokens = Math.max(0, numberValue(segment.input_tokens ?? segment.inputTokens) ?? 0);
    const outputTokens = Math.max(0, numberValue(segment.output_tokens ?? segment.outputTokens) ?? 0);
    const reasoningTokens = Math.max(0, numberValue(segment.reasoning_tokens ?? segment.reasoningTokens) ?? 0);
    summary.totalTokens += inputTokens + outputTokens + reasoningTokens;
  }
  return summary.providerCallCount > 0
    || summary.toolCallCount > 0
    || summary.totalTokens > 0
    || summary.cachedInputTokens > 0
    ? summary
    : null;
}

// subagentAgentStatusText formats Codex-style agent_status values for rows.
// subagentAgentStatusText 为行格式化 Codex 风格的 agent_status 值。
function subagentAgentStatusText(
  // Parsed agent payload.
  // 已解析的代理载荷。
  agent: Record<string, unknown> | null,
): string | null {
  const status = agent?.agent_status ?? agent?.agentStatus;
  if (typeof status === "string" && status.trim()) {
    return status.trim();
  }
  const statusObject = objectValue(status);
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
function subagentAgentStatusResult(
  // Parsed agent payload.
  // 已解析的代理载荷。
  agent: Record<string, unknown> | null,
): string | null {
  const statusObject = objectValue(agent?.agent_status ?? agent?.agentStatus);
  return stringValue(statusObject?.completed) ?? stringValue(statusObject?.errored);
}

// subagentDebugSummary converts creation diagnostics into a compact human-readable line.
// subagentDebugSummary 将创建诊断转换为紧凑的人类可读信息行。
function subagentDebugSummary(
  // Raw creation debug payload.
  // 原始创建调试载荷。
  value: unknown,
): string | null {
  const debug = objectValue(value);
  if (!debug) {
    return null;
  }
  const parts = [
    subagentDebugPart("mode", stringValue(debug.creation_mode ?? debug.creationMode)),
    subagentDebugPart("agent", stringValue(debug.agent_id ?? debug.agentId)),
    debug.has_custom_system_prompt === true || debug.hasCustomSystemPrompt === true
      ? "custom prompt"
      : null,
  ].filter((part): part is string => Boolean(part));
  return parts.length ? parts.join(" · ") : null;
}

// subagentDebugPart formats one named diagnostic value.
// subagentDebugPart 格式化单个带名称的诊断值。
function subagentDebugPart(
  // Label displayed before the diagnostic value.
  // 显示在诊断值之前的标签。
  label: string,
  // Optional diagnostic value.
  // 可选诊断值。
  value: string | null,
): string | null {
  return value ? `${label}: ${value}` : null;
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

// closedAgents reads closed public child-agent ids from control payloads.
// closedAgents 从控制载荷读取已关闭子智能体公开标识。
function closedAgents(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): string[] {
  return arrayValue(payload?.closed_agents ?? payload?.closedAgents)
    .map(stringValue)
    .filter((value): value is string => Boolean(value));
}

// closedAgentCount reads the explicit closed count or falls back to the closed-agent list length.
// closedAgentCount 读取显式关闭数量，缺失时回退为已关闭智能体列表长度。
function closedAgentCount(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): number {
  return numberValue(payload?.closed_count ?? payload?.closedCount) ?? closedAgents(payload).length;
}

// clearedMessageCount reads the number of queued inter-agent messages cleared by close_agent.
// clearedMessageCount 读取 close_agent 清理的代理间排队消息数量。
function clearedMessageCount(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): number | null {
  return numberValue(payload?.cleared_message_count ?? payload?.clearedMessageCount);
}

// stoppedTaskIds reads stopped task ids from control payloads.
// stoppedTaskIds 从控制载荷读取已停止任务标识。
function stoppedTaskIds(
  // Host or body payload.
  // 宿主或正文载荷。
  payload: Record<string, unknown> | null,
): string[] {
  return arrayValue(payload?.stopped_subagent_task_ids ?? payload?.stoppedSubagentTaskIds)
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
  const rawId = blocks.map((block) => block.toolCallId ?? block.id).join(":");
  return `subagent:${operation}:${compactHash(`${rawId}:${title}`)}`;
}
