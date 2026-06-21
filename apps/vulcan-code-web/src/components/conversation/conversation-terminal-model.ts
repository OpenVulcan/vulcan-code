import type { CommandSessionSummary } from "@/types";

// TerminalVisualState is the compact state used by terminal rows and badges.
// TerminalVisualState 是终端行与徽标使用的紧凑状态。
export type TerminalVisualState = "running" | "waiting" | "success" | "failed" | "stopped" | "lost" | "unknown";

// TerminalSessionViewModel describes one command-session row after UI normalization.
// TerminalSessionViewModel 描述 UI 归一化后的单条命令会话行。
export interface TerminalSessionViewModel {
  // Stable command-session identifier.
  // 稳定命令会话标识。
  id: string;
  // Command text shown in terminal summaries.
  // 终端摘要中展示的命令文本。
  command: string;
  // Normalized visual lifecycle state.
  // 归一化后的视觉生命周期状态。
  state: TerminalVisualState;
  // Localized state label shown beside the icon.
  // 图标旁显示的本地化状态文案。
  label: string;
  // Recent preview text returned by the runtime.
  // 运行时返回的最近预览文本。
  preview: string;
  // Optional process exit code.
  // 可选进程退出码。
  exitCode: number | null;
  // Whether the session can be force-stopped from the UI.
  // 该会话是否允许从界面强制停止。
  canStop: boolean;
}

// terminalSessionViewModel normalizes one backend command-session summary for rendering.
// terminalSessionViewModel 将单条后端命令会话摘要归一化为渲染模型。
export function terminalSessionViewModel(summary: CommandSessionSummary): TerminalSessionViewModel {
  const state = terminalVisualState(summary.status, summary.exitCode ?? null, Boolean(summary.lost));
  return {
    id: summary.id,
    command: summary.command || summary.id,
    state,
    label: terminalStateLabel(state),
    preview: summary.preview ?? "",
    exitCode: summary.exitCode ?? null,
    canStop: terminalStateCanStop(state),
  };
}

// terminalSummaryById builds a lookup table for live command-session summaries.
// terminalSummaryById 为实时命令会话摘要构建查找表。
export function terminalSummaryById(summaries: CommandSessionSummary[]): Map<string, CommandSessionSummary> {
  return new Map(summaries.map((summary) => [summary.id, summary]));
}

// terminalVisualState maps raw runtime status text into a stable UI state.
// terminalVisualState 将原始运行时状态文本映射为稳定 UI 状态。
export function terminalVisualState(
  status: string | null | undefined,
  exitCode: number | null,
  lost: boolean,
): TerminalVisualState {
  if (lost) {
    return "lost";
  }
  const normalized = (status ?? "").trim().toLowerCase();
  if (normalized.includes("wait") || normalized.includes("input") || normalized.includes("stdin")) {
    return "waiting";
  }
  if (normalized.includes("run") || normalized.includes("active") || normalized.includes("start")) {
    return "running";
  }
  if (normalized.includes("stop") || normalized.includes("cancel") || normalized.includes("kill") || normalized.includes("terminat")) {
    return "stopped";
  }
  if (normalized.includes("lost") || normalized.includes("missing") || normalized.includes("disconnect")) {
    return "lost";
  }
  if (normalized.includes("fail") || normalized.includes("error")) {
    return "failed";
  }
  if (normalized.includes("complete") || normalized.includes("success") || normalized.includes("done") || normalized.includes("exit")) {
    return exitCode === null || exitCode === 0 ? "success" : "failed";
  }
  if (typeof exitCode === "number") {
    return exitCode === 0 ? "success" : "failed";
  }
  return "unknown";
}

// terminalStateLabel returns the Chinese label for a normalized terminal state.
// terminalStateLabel 返回归一化终端状态对应的中文文案。
export function terminalStateLabel(state: TerminalVisualState): string {
  switch (state) {
    case "running":
      return "运行中";
    case "waiting":
      return "等待输入";
    case "success":
      return "已完成";
    case "failed":
      return "执行失败";
    case "stopped":
      return "已停止";
    case "lost":
      return "已丢失";
    default:
      return "未知";
  }
}

// terminalStateCanStop reports whether the UI may offer a force-stop action.
// terminalStateCanStop 判断界面是否可以提供强制停止动作。
export function terminalStateCanStop(state: TerminalVisualState): boolean {
  return state === "running" || state === "waiting";
}

// terminalStateClass returns the CSS suffix used by terminal rows.
// terminalStateClass 返回终端行使用的 CSS 状态后缀。
export function terminalStateClass(state: TerminalVisualState): string {
  return `terminal-state-${state}`;
}
