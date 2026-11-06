// Labels and tones of subagent run states, shared by the session tool panel and the activity panel.
// 子代理运行状态的标签与色调，由会话工具面板与活动面板共用。
import type { TranslationKey } from "@/i18n";

// conversationSessionSubagentRunStateTone colours a running agent ok and an interrupted one bad.
// conversationSessionSubagentRunStateTone 将运行中的代理标为成功色，被中断的标为错误色。
export function conversationSessionSubagentRunStateTone(
  // Backend run-state key.
  // 后端运行状态键。
  runState: string,
): "ok" | "bad" | undefined {
  if (runState === "running") return "ok";
  if (runState === "failed" || runState === "stopped" || runState === "closed") return "bad";
  return undefined;
}

// conversationSessionSubagentRunStateLabel maps backend run-state keys onto compact localized labels.
// conversationSessionSubagentRunStateLabel 将后端运行状态键映射为紧凑本地化标签。
export function conversationSessionSubagentRunStateLabel(
  // Backend run-state key.
  // 后端运行状态键。
  runState: string,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  switch (runState) {
    case "running":
      return t("conversation.subagent.state.running");
    case "stopped":
      return t("conversation.subagent.state.stopped");
    case "failed":
      return t("conversation.subagent.state.failed");
    case "closed":
      return t("conversation.subagent.state.closed");
    case "completed":
    default:
      return t("conversation.subagent.state.completed");
  }
}
