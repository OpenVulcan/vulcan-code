// Reads one durable V1 tool-call event into the facts every tool card needs: who ran, with what input, how it
// ended, and what the host reported. Cards read these facts only; they never parse transport shapes themselves.
// 将一条可靠 V1 工具调用事件读取为每张工具卡所需的事实：谁运行、输入是什么、如何结束、宿主报告了什么。
// 卡片只读取这些事实，从不自行解析传输结构。
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import type { ToolRequest } from "./tool-request";

// ToolCardState is the visual lifecycle of one card; absent means nothing worth flagging.
// ToolCardState 是单张卡的视觉生命周期；缺省表示无需提示。
export type ToolCardState = "running" | "waiting" | "done" | "failed" | "stopped";

// ToolHostResult is the typed host-only result attached to a tool output envelope.
// ToolHostResult 是附加在工具输出信封上的强类型宿主专用结果。
export interface ToolHostResult {
  // Host result discriminator such as command_run or file_access.
  // 宿主结果判别值，例如 command_run 或 file_access。
  kind: string;
  // Host payload owned by that kind.
  // 该类型拥有的宿主载荷。
  payload: Record<string, unknown>;
}

// ToolCall is the card-facing reading of one tool-call event.
// ToolCall 是一条工具调用事件面向卡片的读取结果。
export interface ToolCall {
  // The exact event, kept for cards that need its change-set or references.
  // 精确事件，供需要其变更集或引用的卡片使用。
  event: ToolCallEvent;
  // Stable key for expansion memory.
  // 展开记忆使用的稳定键。
  key: string;
  // Tool name in canonical comparison form.
  // 规范比较形式的工具名。
  name: string;
  // Arguments the model supplied.
  // 模型提供的参数。
  input: Record<string, unknown>;
  // Text the model received back, when the call finished.
  // 调用结束后模型收到的文本。
  modelOutput: string | null;
  // Host-only structured result, when the tool produced one.
  // 工具产生的宿主专用结构化结果。
  host: ToolHostResult | null;
  // Whether the tool reported an error result.
  // 工具是否报告了错误结果。
  isError: boolean;
  // Visual lifecycle.
  // 视觉生命周期。
  state: ToolCardState;
  // Wall-clock time between start and finish, when both are known.
  // 开始与结束之间的实际耗时（两者均已知时）。
  durationMs: number | null;
  // The request while the model is still writing it; null once the call exists.
  // 模型仍在书写时的请求；调用存在后为 null。
  request: ToolRequest | null;
}

// readToolCall reads one event; it never throws on unexpected payloads.
// readToolCall 读取一条事件；遇到意外载荷时不会抛出。
export function readToolCall(event: ToolCallEvent): ToolCall {
  const output = event.output ?? null;
  const host = objectOf(output?.host_result);
  const hostPayload = host ? objectOf(host.payload) : null;
  const isError = output?.is_error === true;
  return {
    durationMs: event.started_at_ms !== undefined && event.finished_at_ms !== undefined
      ? Math.max(0, event.finished_at_ms - event.started_at_ms)
      : null,
    event,
    host: host && typeof host.kind === "string" ? { kind: host.kind, payload: hostPayload ?? {} } : null,
    input: event.input,
    isError,
    key: `${event.api_call_id}:${event.tool_call_id}`,
    modelOutput: typeof output?.model_output === "string" ? output.model_output : null,
    name: canonicalToolName(event.tool_name),
    request: null,
    state: toolCardState(event.status, isError),
  };
}

// canonicalToolName lower-cases a name and joins its words with underscores.
// canonicalToolName 将名称转为小写并以下划线连接单词。
export function canonicalToolName(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "");
}

// toolCardState maps the durable lifecycle onto the card's visual state.
// toolCardState 将可靠生命周期映射为卡片视觉状态。
export function toolCardState(status: ToolCallEvent["status"], isError: boolean): ToolCardState {
  switch (status) {
    case "prepared":
    case "running":
      return "running";
    case "waiting_user":
      return "waiting";
    case "completed":
      return isError ? "failed" : "done";
    case "failed":
      return "failed";
    case "cancelled":
    case "stopped":
    case "interrupted":
      return "stopped";
  }
}

// objectOf returns a plain object, or null for anything else.
// objectOf 返回普通对象；其他值返回 null。
export function objectOf(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

// textOf returns a non-empty string, or null.
// textOf 返回非空字符串；否则返回 null。
export function textOf(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

// numberOf returns a finite number, or null.
// numberOf 返回有限数字；否则返回 null。
export function numberOf(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

// listOf returns an array, or an empty array for anything else.
// listOf 返回数组；其他值返回空数组。
export function listOf(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

// SLOW_TOOL_CALL_MS is how long a call must take before its card keeps showing the duration after it ends.
// SLOW_TOOL_CALL_MS 是调用结束后卡片仍持续显示耗时所需的最短调用时长。
export const SLOW_TOOL_CALL_MS = 2000;

// formatToolDuration writes a call duration at the precision a reader needs: milliseconds under a second, tenths
// under a minute, then minutes and seconds.
// formatToolDuration 按读者所需精度书写调用耗时：一秒内用毫秒，一分钟内精确到十分之一秒，之后用分与秒。
export function formatToolDuration(milliseconds: number): string {
  const ms = Math.max(0, milliseconds);
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const seconds = Math.round(ms / 1000);
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
}
