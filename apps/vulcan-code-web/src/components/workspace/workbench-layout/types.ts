import type {
  WorkbenchDeltaEnvelope,
  WorkbenchSnapshot,
} from "@/types";

// ThemeMode describes the two shadcn default theme modes.
// ThemeMode 描述 shadcn 默认的两种主题模式。
export type ThemeMode = "light" | "dark";

// ConversationHistoryCursor stores the backend cursor for one loaded conversation window.
// ConversationHistoryCursor 保存单个已加载会话窗口的后端历史游标。
export interface ConversationHistoryCursor {
  // earliestOrdinal points to the oldest loaded transcript row.
  // earliestOrdinal 指向当前已加载的最早转录行。
  earliestOrdinal: number | null;
  // hasMoreHistory indicates whether an older page can be requested.
  // hasMoreHistory 表示是否还能请求更早历史页。
  hasMoreHistory: boolean;
  // loading marks an in-flight older-page request for this conversation.
  // loading 标记该会话是否正在请求更早历史页。
  loading: boolean;
}

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

// conversationHistoryCursorFromWindow converts one service-retained body window into UI cursor state.
// conversationHistoryCursorFromWindow 将单个服务端保留正文窗口转换为 UI 游标状态。
export function conversationHistoryCursorFromWindow(
  window: NonNullable<WorkbenchSnapshot["hydratedConversationWindows"]>[number],
  loading: boolean,
): ConversationHistoryCursor {
  return {
    earliestOrdinal: window.earliestOrdinal ?? null,
    hasMoreHistory: window.hasMoreHistory,
    loading,
  };
}

// initialThemeMode returns the persisted shadcn theme, defaulting to light.
// initialThemeMode 返回持久化的 shadcn 主题，默认使用亮色。
export function initialThemeMode(): ThemeMode {
  const stored = window.localStorage.getItem("vulcan-webui-theme");
  return stored === "light" || stored === "dark" ? stored : "light";
}

// webuiClickTrace emits lightweight browser-console diagnostics for user initiated WebUI actions.
// webuiClickTrace 为用户触发的 WebUI 操作输出轻量浏览器控制台诊断日志。
export function webuiClickTrace(_action: string, _detail: Record<string, unknown> = {}): void {
  // Kept as a no-op compatibility hook so existing click paths stay unchanged without emitting dev logs.
  // 保留为空实现兼容钩子，避免改变现有点击路径，同时不再输出开发日志。
}

// copyText writes transcript detail text to the system clipboard when available.
// copyText 在可用时将转录详情文本写入系统剪贴板。
export async function copyText(text: string): Promise<void> {
  await navigator.clipboard?.writeText(text);
}
