// Chooses the card for one tool call by what the call did, and gives the sequence summary the same family.
// 按调用实际做的事选择工具卡，并让序列摘要使用同一家族。
import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { AgentCard, agentToolSpec } from "./cards/agent-card";
import { AskCard } from "./cards/ask-card";
import { ShellCommandCard, TerminalSessionCard, toolCallIsShellCommand } from "./cards/command-card";
import { FileChangeCard, FileInspectCard, FileReadCard, MediaReadCard } from "./cards/file-cards";
import { DocumentCard, documentToolSpec } from "./cards/document-card";
import { GenericToolCard } from "./cards/generic-card";
import { KnowledgeWriteCard } from "./cards/knowledge-write-card";
import { MemorySearchCard } from "./cards/memory-search-card";
import { LUA_TOOLS, LuaCard } from "./cards/lua-card";
import { GoalCard, PlanCard, TodoCard } from "./cards/plan-cards";
import { ProviderCard, providerToolSpec } from "./cards/provider-card";
import type { ToolCardProps, ToolFamily } from "./tool-card";
import { readToolCall, textOf, type ToolCall, type ToolCardState } from "./tool-call";
import { readRequestPreview, requestToolCall, type ToolRequestPhase } from "./tool-request";

// ToolCardDefinition pairs a call's family with the card that draws it.
// ToolCardDefinition 将调用的家族与绘制它的卡片配对。
export interface ToolCardDefinition {
  // Purpose family.
  // 用途家族。
  family: ToolFamily;
  // Card component.
  // 卡片组件。
  Card: React.ComponentType<ToolCardProps>;
}

// FILE_MUTATION_TOOLS change files even when a failed call carries no change-set.
// FILE_MUTATION_TOOLS 即使失败调用不携带变更集也属于文件修改。
const FILE_MUTATION_TOOLS = new Set(["file_edit", "file_write", "apply_patch"]);

// GOAL_TOOLS read or change the session goal.
// GOAL_TOOLS 读取或修改会话目标。
const GOAL_TOOLS = new Set(["get_goal", "create_goal", "update_goal"]);

// FILE_INSPECT_ACTIONS are the file tool actions that only look.
// FILE_INSPECT_ACTIONS 是 file 工具中只读取的动作。
const FILE_INSPECT_ACTIONS = new Set(["list", "stat"]);

/** Resolves the purpose-built card for one call, or null when its tool has none yet.
 * 为一次调用解析专用卡片；其工具尚无专用卡时返回 null。 */
export function resolvePurposeCard(call: ToolCall): ToolCardDefinition | null {
  if (call.event.changeset || FILE_MUTATION_TOOLS.has(call.name)) return { Card: FileChangeCard, family: "edit" };
  if (call.name === "file_read" || call.host?.kind === "file_access") return { Card: FileReadCard, family: "read" };
  if (call.name === "read_media") return { Card: MediaReadCard, family: "read" };
  if (call.name === "file") {
    const action = textOf(call.input.action) ?? "";
    if (action === "read") return { Card: FileReadCard, family: "read" };
    return FILE_INSPECT_ACTIONS.has(action)
      ? { Card: FileInspectCard, family: "read" }
      : { Card: FileChangeCard, family: "edit" };
  }
  if (toolCallIsShellCommand(call)) return { Card: ShellCommandCard, family: "run" };
  if (call.name === "terminal_session" || call.host?.kind === "terminal_session") return { Card: TerminalSessionCard, family: "run" };
  if (LUA_TOOLS.has(call.name)) return { Card: LuaCard, family: "run" };
  if (call.name === "todolist") return { Card: TodoCard, family: "plan" };
  if (GOAL_TOOLS.has(call.name)) return { Card: GoalCard, family: "plan" };
  if (call.name === "submit_plan") return { Card: PlanCard, family: "plan" };
  if (call.name === "ask") return { Card: AskCard, family: "agent" };
  if (agentToolSpec(call)) return { Card: AgentCard, family: "agent" };
  if (call.name === "knowledge_write") return { Card: KnowledgeWriteCard, family: "edit" };
  if (call.name === "vmm_memory_search") return { Card: MemorySearchCard, family: "search" };
  const provider = providerToolSpec(call);
  if (provider) return { Card: ProviderCard, family: provider.family };
  const document = documentToolSpec(call);
  if (document) return { Card: DocumentCard, family: document.family };
  return null;
}

// GENERIC_CARD draws every call without a purpose-built card.
// GENERIC_CARD 绘制所有没有专用卡的调用。
const GENERIC_CARD: ToolCardDefinition = { Card: GenericToolCard, family: "other" };

/** Resolves the card for one call; tools without a purpose-built card use the generic card.
 * 为一次调用解析卡片；没有专用卡的工具使用通用卡。 */
export function resolveToolCard(call: ToolCall): ToolCardDefinition {
  return resolvePurposeCard(call) ?? GENERIC_CARD;
}

// toolCallFamily returns the family of one event without rendering it.
// toolCallFamily 返回一条事件的家族而不渲染它。
export function toolCallFamily(event: ToolCallEvent): ToolFamily {
  return resolveToolCard(readToolCall(event)).family;
}

/** Renders the card for one tool-call event.
 * 渲染一条工具调用事件的卡片。 */
export const ToolCallCard = React.memo(function ToolCallCard(props: {
  // Durable tool-call event.
  // 可靠工具调用事件。
  event: ToolCallEvent;
  // Whether the card body is open.
  // 卡片正文是否展开。
  open: boolean;
  // Requests the next open state.
  // 请求下一个展开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const call = React.useMemo(() => readToolCall(props.event), [props.event]);
  const { Card, family } = resolveToolCard(call);
  return <Card call={call} family={family} onOpenChange={props.onOpenChange} open={props.open} />;
});

/** Renders the card for a tool request the model is still writing, or one the turn ended before it became a call;
 * it is the same card the finished call will use, so the row keeps its shape when the call arrives.
 * 渲染模型仍在书写的工具请求（或回合在其成为调用前就结束的请求）的卡片；它与最终调用使用同一张卡，
 * 使调用到达时该行保持原样。 */
export const ToolRequestCard = React.memo(function ToolRequestCard(props: {
  // Public tool name frozen for the request.
  // 请求冻结的公开工具名。
  toolName: string;
  // Stable request stream identity, used as the call key.
  // 稳定的请求流标识，用作调用键。
  requestStreamId: string;
  // Writing phase.
  // 书写阶段。
  phase: ToolRequestPhase;
  // UTF-8 bytes received so far.
  // 目前已接收的 UTF-8 字节数。
  receivedBytes: number;
  // Host preview object, when the tool produces one.
  // 宿主预览对象（仅产生预览的工具）。
  preview: Record<string, unknown> | null;
  // Visual lifecycle of the row.
  // 该行的视觉生命周期。
  state: ToolCardState;
  // Whether the card body is open.
  // 卡片正文是否展开。
  open: boolean;
  // Requests the next open state.
  // 请求下一个展开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const preview = React.useMemo(() => readRequestPreview(props.preview), [props.preview]);
  const call = React.useMemo(
    () => requestToolCall(props.toolName, { bytes: props.receivedBytes, phase: props.phase, preview }, props.requestStreamId, props.state),
    [preview, props.phase, props.receivedBytes, props.requestStreamId, props.state, props.toolName],
  );
  const { Card, family } = resolveToolCard(call);
  return <Card call={call} family={family} onOpenChange={props.onOpenChange} open={props.open} />;
});
