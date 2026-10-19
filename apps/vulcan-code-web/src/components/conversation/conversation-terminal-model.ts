import type { CommandSessionSummary } from "@/types";
import type { TranslationKey } from "@/i18n-messages";

// TerminalVisualState is the compact state used by terminal rows and badges.
// TerminalVisualState 是终端行与徽标使用的紧凑状态。
export type TerminalVisualState = "running" | "waiting" | "success" | "failed" | "stopped" | "lost" | "unknown";

// TerminalSessionViewModel describes one command-session row after UI normalization.
// TerminalSessionViewModel 描述 UI 归一化后的单条命令会话行。
export interface TerminalSessionViewModel {
  // Stable command-session identifier.
  // 稳定命令会话标识。
  id: string;
  // User-facing terminal session name.
  // 用户可见终端会话名称。
  name: string;
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

// shortenTerminalSessionId keeps long terminal ids readable in compact UI.
// shortenTerminalSessionId 让较长终端标识在紧凑界面中保持可读。
export function shortenTerminalSessionId(value: string): string {
  const trimmed = value.trim();
  if (trimmed.length <= 8) {
    return trimmed;
  }
  return trimmed.slice(0, 8);
}

// terminalSessionViewModel normalizes one backend command-session summary for rendering.
// terminalSessionViewModel 将单条后端命令会话摘要归一化为渲染模型。
export function terminalSessionViewModel(summary: CommandSessionSummary, t?: (key: TranslationKey) => string): TerminalSessionViewModel {
  const state = terminalVisualState(summary.status, summary.exitCode ?? null, Boolean(summary.lost));
  return {
    id: summary.id,
    name: (summary.name ?? "").trim() || shortenTerminalSessionId(summary.id),
    command: summary.command || summary.id,
    state,
    label: terminalStateLabel(state, t),
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

// terminalVisualState maps the backend command-session lifecycle label into a stable UI state.
// terminalVisualState 将后端命令会话生命周期标签映射为稳定 UI 状态。
export function terminalVisualState(
  status: string | null | undefined,
  exitCode: number | null,
  lost: boolean,
): TerminalVisualState {
  if (lost) {
    return "lost";
  }
  const normalized = (status ?? "").trim().toLowerCase();
  switch (normalized) {
    case "running":
      return "running";
    case "completed":
      return exitCode === null || exitCode === 0 ? "success" : "failed";
    case "failed":
      return "failed";
    case "stopped":
      return "stopped";
    default:
      return "unknown";
  }
}

// terminalStateLabel returns the localized label for a normalized terminal state.
// terminalStateLabel 返回归一化终端状态对应的本地化文案。
export function terminalStateLabel(state: TerminalVisualState, t?: (key: TranslationKey) => string): string {
  switch (state) {
    case "running":
      return t ? t("terminalState.running") : "Running";
    case "waiting":
      return t ? t("terminalState.waiting") : "Waiting for input";
    case "success":
      return t ? t("terminalState.success") : "Completed";
    case "failed":
      return t ? t("terminalState.failed") : "Failed";
    case "stopped":
      return t ? t("terminalState.stopped") : "Stopped";
    case "lost":
      return t ? t("terminalState.lost") : "Lost";
    default:
      return t ? t("terminalState.unknown") : "Unknown";
  }
}

// terminalStateCanStop reports whether the UI may offer a force-stop action.
// terminalStateCanStop 判断界面是否可以提供强制停止动作。
export function terminalStateCanStop(state: TerminalVisualState): boolean {
  return state === "running" || state === "waiting";
}

// terminalStateTone maps a terminal state onto the kit tone of its mark; settled or unknown states stay neutral.
// terminalStateTone 将终端状态映射为其标记的 kit 色调；已停止或未知状态保持中性。
export function terminalStateTone(state: TerminalVisualState): "info" | "warn" | "ok" | "bad" | undefined {
  if (state === "running") return "info";
  if (state === "waiting") return "warn";
  if (state === "success") return "ok";
  if (state === "failed") return "bad";
  return undefined;
}
