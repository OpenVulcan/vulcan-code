import type { TranslationKey } from "@/i18n";
import type {
  WorkbenchDeltaEnvelope,
} from "@/types";

// WorkbenchDeltaSignal wraps the newest partial update with a React-visible sequence.
// WorkbenchDeltaSignal 使用 React 可见序号包装最新局部更新。
export interface WorkbenchDeltaSignal {
  // Monotonic frontend sequence that makes repeated delta objects observable.
  // 前端单调序号，用于让重复增量对象也能被观察到。
  seq: number;
  // Backend partial workbench update envelopes batched for one render frame.
  // 单个渲染帧内批处理的后端局部工作台更新信封。
  envelopes: WorkbenchDeltaEnvelope[];
}

// StopConversationRunChoice is the user decision for running child agents.
// StopConversationRunChoice 是用户对运行中智能体的停止决策。
export type StopConversationRunChoice = "main_only" | "main_and_subagents";

// StopConversationRunAgentPromptItem is shown in the stop confirmation dialog.
// StopConversationRunAgentPromptItem 展示在停止确认对话框中。
export interface StopConversationRunAgentPromptItem {
  // Stable child task session id.
  // 稳定的子任务会话标识。
  taskId: string;
  // Human-facing child agent label.
  // 面向用户的智能体标签。
  label: string;
}

// RequestStopConversationRunChoice asks the user how to handle running child agents.
// RequestStopConversationRunChoice 询问用户如何处理运行中的智能体。
export type RequestStopConversationRunChoice = (
  // Persisted conversation captured before the stop preflight request.
  // 停止预检请求前捕获的已持久化会话。
  conversationId: string,
  // Running child agents reported for that exact conversation tree.
  // 当前精确会话树报告的运行中子代理。
  agents: StopConversationRunAgentPromptItem[],
) => Promise<StopConversationRunChoice | null>;

// webuiClickTrace emits lightweight browser-console diagnostics for user initiated WebUI actions.
// webuiClickTrace 为用户触发的 WebUI 操作输出轻量浏览器控制台诊断日志。
export function webuiClickTrace(_action: string, _detail: Record<string, unknown> = {}): void {
  // Kept as a no-op compatibility hook so existing click paths stay unchanged without emitting dev logs.
  // 保留为空实现兼容钩子，避免改变现有点击路径，同时不再输出开发日志。
}

// copyText writes the supplied transcript text and rejects when the clipboard cannot accept it.
// copyText 写入传入的转录文本，剪贴板无法接收时明确拒绝，以便调用方显示失败。
export async function copyText(text: string): Promise<void> {
  if (typeof navigator.clipboard?.writeText !== "function") {
    throw new Error("Clipboard writing is unavailable in this client.");
  }
  await navigator.clipboard.writeText(text);
}

// formatWorkbenchTranslation replaces named placeholders in localized workbench messages.
// formatWorkbenchTranslation 替换工作台本地化消息中的具名占位符。
export function formatWorkbenchTranslation(
  t: (key: TranslationKey) => string,
  key: TranslationKey,
  values: Record<string, string>,
): string {
  return Object.entries(values).reduce(
    (text, [name, value]) => text.replaceAll(`{${name}}`, value),
    t(key),
  );
}
