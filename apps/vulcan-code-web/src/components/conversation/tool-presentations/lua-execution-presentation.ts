import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import type { LuaExecutionPresentation, ToolPresentationDisplayState } from "./types";
import {
  argumentsFromMetadata,
  compactHash,
  fixedHostKind,
  fixedHostPayload,
  fixedToolName,
  numberValue,
  objectValue,
  stringValue,
  textValue,
} from "./presentation-value-readers";

// RunLuaInvocationArguments contains the exact display fields declared by the two public tools.
// RunLuaInvocationArguments 包含两个公开工具声明的精确展示字段。
interface RunLuaInvocationArguments {
  // Exact lifecycle action, eval for one-shot execution, or invalid for malformed session calls.
  // 精确生命周期动作；一次性执行为 eval，畸形会话调用为 invalid。
  action: LuaExecutionPresentation["action"];
  // Optional short task or session name.
  // 可选简短任务或会话名称。
  label: string | null;
  // Optional complete inline Lua source.
  // 可选完整内联 Lua 源码。
  code: string | null;
  // Optional persistent lease identifier.
  // 可选持久租约标识。
  sessionId: string | null;
  // Exact structured args object used only by the dedicated display table.
  // 仅供专用展示表格使用的精确结构化 args 对象。
  arguments: Record<string, unknown> | null;
}

// hydrateLuaExecutionPresentation merges the hidden model-facing result into the Lua card.
// hydrateLuaExecutionPresentation 将隐藏的面向模型结果合并到 Lua 卡片。
export function hydrateLuaExecutionPresentation(
  presentation: LuaExecutionPresentation,
  payload: ToolDetailPayload | null,
): LuaExecutionPresentation {
  if (!payload?.body.trim()) {
    return presentation;
  }
  return {
    ...presentation,
    output: payload.body,
    state: presentation.toolId === "runlua" && isRunLuaErrorOutput(payload.body)
      ? "failed"
      : presentation.state,
  };
}

// resolveLuaExecutionPresentation recognizes only the exact RunLua tool ids and host result kinds.
// resolveLuaExecutionPresentation 只识别精确 RunLua 工具标识与宿主结果类型。
export function resolveLuaExecutionPresentation(
  blocks: ToolExecutionConversationBlock[],
  displayState: ToolPresentationDisplayState,
  detailRef?: ConversationDetailRef,
): LuaExecutionPresentation | null {
  const toolName = fixedToolName(blocks);
  const hostKind = fixedHostKind(blocks);
  // ToolStarted has the exact tool id while ToolFinished additionally has the exact host kind.
  // ToolStarted 携带精确工具标识，ToolFinished 还会携带精确宿主结果类型。
  const toolId = toolName === "runlua" || hostKind === "lua_execution"
    ? "runlua"
    : toolName === "runlua_session" || hostKind === "lua_runtime_session"
      ? "runlua_session"
      : null;
  if (!toolId) {
    return null;
  }
  const invocationArguments = firstRunLuaInvocationArguments(blocks, toolId);
  const hostPayload = fixedHostPayload(blocks);
  const action = runLuaAction(hostPayload, invocationArguments, toolId);
  if (!action) {
    return null;
  }
  const label = invocationArguments?.label
    ?? stringValue(hostPayload?.task)
    ?? stringValue(hostPayload?.name);
  const code = invocationArguments?.code
    ?? textValue(hostPayload?.codePreview);
  const sessionId = invocationArguments?.sessionId
    ?? stringValue(hostPayload?.sessionId);
  // semanticState preserves transport failures and recognizes both exact host and one-shot error contracts.
  // semanticState 保留传输失败，并识别精确宿主错误与一次性错误契约。
  const output = visibleRunLuaOutput(blocks);
  const semanticState = stringValue(hostPayload?.status) === "failed"
    || (toolId === "runlua" && isRunLuaErrorOutput(output))
    ? "failed"
    : displayState;
  return {
    action,
    arguments: invocationArguments?.arguments ?? null,
    code,
    detailRef,
    elapsedMs: numberValue(hostPayload?.elapsedMs),
    id: luaExecutionPresentationId(blocks, toolId, action),
    kind: "lua_execution",
    label,
    mode: toolId === "runlua" ? "one_shot" : "persistent",
    output,
    sessionId,
    state: semanticState,
    toolId,
    ttlSec: numberValue(hostPayload?.ttlSec),
  };
}

// firstRunLuaInvocationArguments reads the fixed argument object from the visible ToolStarted block.
// firstRunLuaInvocationArguments 从可见 ToolStarted 块读取固定参数对象。
function firstRunLuaInvocationArguments(
  blocks: ToolExecutionConversationBlock[],
  toolId: LuaExecutionPresentation["toolId"],
): RunLuaInvocationArguments | null {
  for (const block of blocks) {
    const args = objectValue(argumentsFromMetadata(block.rawMetadata));
    if (!args) {
      continue;
    }
    if (toolId === "runlua") {
      return {
        action: "eval",
        arguments: structuredRunLuaArguments(args.args),
        code: textValue(args.code),
        label: stringValue(args.task),
        sessionId: null,
      };
    }
    // invalid is explicit display state for malformed calls whose exact lifecycle action is absent or unsupported.
    // invalid 是缺少或不支持精确生命周期动作的畸形调用所使用的显式展示状态。
    const action = persistentRunLuaAction(args.action) ?? "invalid";
    return {
      action,
      arguments: structuredRunLuaArguments(args.args),
      code: textValue(args.code),
      label: stringValue(args.name),
      sessionId: stringValue(args.session_id),
    };
  }
  return null;
}

// runLuaAction resolves the exact lifecycle action from the host payload or invocation contract.
// runLuaAction 从宿主载荷或调用契约解析精确生命周期动作。
function runLuaAction(
  hostPayload: Record<string, unknown> | null,
  invocationArguments: RunLuaInvocationArguments | null,
  toolId: LuaExecutionPresentation["toolId"],
): LuaExecutionPresentation["action"] | null {
  return persistentRunLuaAction(hostPayload?.action)
    ?? invocationArguments?.action
    ?? (toolId === "runlua" ? "eval" : "invalid");
}

// persistentRunLuaAction validates one value against the public session action enum.
// persistentRunLuaAction 按公开会话动作枚举校验一个值。
function persistentRunLuaAction(value: unknown): LuaExecutionPresentation["action"] | null {
  const action = stringValue(value);
  if (action === "eval" || action === "list" || action === "start" || action === "status" || action === "close") {
    return action;
  }
  return null;
}

// visibleRunLuaOutput returns only inline result bodies, never the invocation argument body.
// visibleRunLuaOutput 只返回内联结果正文，绝不返回调用参数正文。
function visibleRunLuaOutput(blocks: ToolExecutionConversationBlock[]): string {
  return blocks
    .filter((block) => block.kind === "tool-result")
    .map((block) => block.body)
    .find((body) => body.trim())
    ?? "";
}

// structuredRunLuaArguments copies the exact JSON object declared by the public tool contract.
// structuredRunLuaArguments 复制公开工具契约声明的精确 JSON 对象。
function structuredRunLuaArguments(value: unknown): Record<string, unknown> | null {
  const argumentsObject = objectValue(value);
  return argumentsObject ? { ...argumentsObject } : null;
}

// isRunLuaErrorOutput recognizes the exact LuaSkills 0.5.2 one-shot error document heading.
// isRunLuaErrorOutput 识别 LuaSkills 0.5.2 一次性错误文档的精确标题。
function isRunLuaErrorOutput(output: string): boolean {
  return output.trimStart().startsWith("# Runtime Execution Error");
}

// luaExecutionPresentationId builds a stable expansion key from the fixed tool call identity.
// luaExecutionPresentationId 根据固定工具调用身份构建稳定展开键。
function luaExecutionPresentationId(
  blocks: ToolExecutionConversationBlock[],
  toolId: LuaExecutionPresentation["toolId"],
  action: LuaExecutionPresentation["action"],
): string {
  const identity = blocks
    .map((block) => block.toolCallId ?? block.id)
    .filter(Boolean)
    .join(":");
  return `lua:${toolId}:${action}:${compactHash(identity || toolId)}`;
}
