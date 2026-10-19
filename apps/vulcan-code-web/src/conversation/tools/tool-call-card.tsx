// Chooses the card for one tool call by what the call did, and gives the sequence summary the same family.
// 按调用实际做的事选择工具卡，并让序列摘要使用同一家族。
import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ShellCommandCard, TerminalSessionCard, toolCallIsShellCommand } from "./cards/command-card";
import { FileChangeCard, FileInspectCard, FileReadCard } from "./cards/file-cards";
import { DocumentCard, documentToolSpec } from "./cards/document-card";
import { GenericToolCard } from "./cards/generic-card";
import { GoalCard, PlanCard, TodoCard } from "./cards/plan-cards";
import type { ToolCardProps, ToolFamily } from "./tool-card";
import { readToolCall, textOf, type ToolCall } from "./tool-call";

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
  if (call.name === "file") {
    const action = textOf(call.input.action) ?? "";
    if (action === "read") return { Card: FileReadCard, family: "read" };
    return FILE_INSPECT_ACTIONS.has(action)
      ? { Card: FileInspectCard, family: "read" }
      : { Card: FileChangeCard, family: "edit" };
  }
  if (toolCallIsShellCommand(call)) return { Card: ShellCommandCard, family: "run" };
  if (call.name === "terminal_session" || call.host?.kind === "terminal_session") return { Card: TerminalSessionCard, family: "run" };
  if (call.name === "todolist") return { Card: TodoCard, family: "plan" };
  if (GOAL_TOOLS.has(call.name)) return { Card: GoalCard, family: "plan" };
  if (call.name === "submit_plan") return { Card: PlanCard, family: "plan" };
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

// toolCallHasPurposeCard reports whether the new transcript already owns a purpose-built card for this call.
// toolCallHasPurposeCard 报告新转录是否已为该调用提供专用卡。
export function toolCallHasPurposeCard(event: ToolCallEvent): boolean {
  return resolvePurposeCard(readToolCall(event)) !== null;
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
