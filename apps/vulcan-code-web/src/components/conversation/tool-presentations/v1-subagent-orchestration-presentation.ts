import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { resolveSubagentOrchestrationPresentation } from "@/components/conversation/tool-presentations/subagent-orchestration-presentation";
import type {
  SubagentOrchestrationPresentation,
  ToolPresentationDisplayState,
} from "@/components/conversation/tool-presentations/types";
import type {
  ToolCallDisplayItem,
  ToolCallEvent,
} from "@/service/conversation-event-display-projection";
import type { ToolJsonValue } from "@/service/conversation-event-window-store";

/** Resolves one event-native V1 subagent tool through the established rich orchestration card model.
 * 通过既有富调度卡片模型解析一条事件原生 V1 子代理工具。 */
export function resolveV1SubagentOrchestrationPresentation(
  // Authoritative V1 tool event projected for the transcript.
  // 为转录投影的权威 V1 工具事件。
  item: ToolCallDisplayItem,
): SubagentOrchestrationPresentation | null {
  // toolCall is the only source for arguments, persisted output, and external identity.
  // toolCall 是参数、持久化输出和外部身份的唯一来源。
  const toolCall = item.tool_call;
  // modelOutput is the exact model-visible member persisted by persistent_tool_output_from_result.
  // modelOutput 是 persistent_tool_output_from_result 持久化的精确模型可见成员。
  const modelOutput = toolCall.output?.model_output;
  // visibleBody preserves structured output for the established presentation parser.
  // visibleBody 为既有展示解析器保留结构化输出。
  const visibleBody = v1ToolJsonText(modelOutput);
  // subagentTaskId comes only from the typed V1 external-resource reference.
  // subagentTaskId 仅来自强类型 V1 外部资源引用。
  const subagentTaskId = toolCall.external_refs
    ?.find((reference) => reference.kind === "subagent")
    ?.id;
  // block adapts the fixed V1 persistence envelope without inventing fallback field paths.
  // block 适配固定 V1 持久化信封，不发明兜底字段路径。
  const block: ToolExecutionConversationBlock = {
    id: `v1-tool-${item.seq}`,
    kind: v1ToolStatusIsTerminal(toolCall.status) ? "tool-result" : "tool-call",
    title: toolCall.tool_name,
    body: visibleBody,
    summary: visibleBody,
    status: toolCall.status,
    toolName: toolCall.tool_name,
    toolCallId: String(toolCall.tool_call_id),
    subagentTaskId,
    rawMetadata: {
      arguments: toolCall.input,
      tool_name: toolCall.tool_name,
      host_result: toolCall.output?.host_result,
    },
  };

  return resolveSubagentOrchestrationPresentation(
    [block],
    v1SubagentDisplayState(toolCall.status),
  );
}

/** Converts one persisted V1 tool status to the shared rich-card display state.
 * 将一个持久化 V1 工具状态转换为共享富卡片展示状态。 */
function v1SubagentDisplayState(
  // Exact persisted status.
  // 精确持久化状态。
  status: ToolCallEvent["status"],
): ToolPresentationDisplayState {
  if (status === "completed") {
    return "completed";
  }
  if (status === "failed") {
    return "failed";
  }
  if (status === "cancelled" || status === "stopped" || status === "interrupted") {
    return "stopped";
  }
  return "running";
}

/** Returns whether one persisted V1 tool status is terminal.
 * 返回一个持久化 V1 工具状态是否为终态。 */
function v1ToolStatusIsTerminal(
  // Exact persisted status.
  // 精确持久化状态。
  status: ToolCallEvent["status"],
): boolean {
  return status === "completed"
    || status === "failed"
    || status === "cancelled"
    || status === "stopped"
    || status === "interrupted";
}

/** Serializes one persisted tool JSON value for the legacy-compatible semantic parser.
 * 为兼容既有语义解析器序列化一个持久化工具 JSON 值。 */
function v1ToolJsonText(
  // Model-visible value from the fixed V1 output envelope.
  // 来自固定 V1 输出信封的模型可见值。
  value: ToolJsonValue | undefined,
): string {
  if (value === undefined) {
    return "";
  }
  if (typeof value === "string") {
    return value;
  }
  return JSON.stringify(value);
}
