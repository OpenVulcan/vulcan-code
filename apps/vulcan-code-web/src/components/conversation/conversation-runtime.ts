import type { useI18n } from "@/i18n";
import type { ConversationSummary } from "@/types";
import type {
  RuntimeCue,
} from "@/components/conversation/conversation-model";

// conversationRuntimeCue resolves the compact active-state cue shown above the composer.
// conversationRuntimeCue 解析显示在输入区上方的紧凑运行状态提示。
// Takes the selected conversation, translator and Core request clocks. Returns null when no cue applies.
// 接收选中的会话、翻译函数与 Core 请求起点。无对应提示时返回 null。
export function conversationRuntimeCue(
  conversation: ConversationSummary | undefined,
  t: ReturnType<typeof useI18n>["t"],
  activeRequestStartedAtMs: number | null,
): RuntimeCue | null {
  if (!conversation) {
    return null;
  }
  if (conversation.status.toLowerCase() !== "running") {
    return null;
  }
  const phaseCue = sessionRunPhaseCue(
    conversation.runPhase, t, activeRequestStartedAtMs, conversation.toolRequestStartedAtMs ?? null,
  );
  if (phaseCue) {
    return phaseCue;
  }
  return null;
}

// conversationIsRunning derives the send-button stop state from backend session status.
// conversationIsRunning 从后端会话状态推导发送按钮的停止状态。
export function conversationIsRunning(conversation: ConversationSummary | undefined): boolean {
  if (!conversation) {
    return false;
  }
  const phase = normalizedSessionRunPhase(conversation.runPhase);
  if (phase && ["idle", "ready", "failed", "stopped"].includes(phase)) {
    return false;
  }
  if (phase) {
    return true;
  }
  return conversation.status.trim().toLowerCase() === "running";
}

// normalizedSessionRunPhase normalizes backend enum text and future route-style phase keys.
// normalizedSessionRunPhase 规范化后端枚举文本以及未来路由式阶段键。
export function normalizedSessionRunPhase(runPhase?: string | null): string | null {
  const normalized = runPhase?.trim().toLowerCase().replace(/[-_]/g, "");
  return normalized && normalized.length > 0 ? normalized : null;
}

// sessionRunPhaseCue maps the backend per-session phase to a reusable active cue.
// sessionRunPhaseCue 将后端会话级阶段映射为可复用的活动提示。
// Takes phase text, a translator, and the model and tool request clocks. Returns a cue or null.
// 接收阶段文本、翻译函数及模型与工具请求起点，返回提示或 null。
function sessionRunPhaseCue(
  runPhase: string | null | undefined,
  t: ReturnType<typeof useI18n>["t"],
  activeRequestStartedAtMs: number | null,
  toolRequestStartedAtMs: number | null,
): RuntimeCue | null {
  const normalized = normalizedSessionRunPhase(runPhase);
  if (!normalized || normalized === "idle" || normalized === "ready" || normalized === "failed" || normalized === "stopped") {
    return null;
  }
  if (normalized === "memoryrecall") {
    return { kind: "default", text: t("runtime.memoryRecall") };
  }
  if (normalized === "thinking") {
    return null;
  }
  if (normalized === "toolcalling") {
    return { kind: "default", text: t("runtime.toolCalling"), startedAtMs: toolRequestStartedAtMs };
  }
  if (normalized === "toolrunning") {
    return null;
  }
  return {
    kind: "default",
    text: t("runtime.requesting"),
    startedAtMs: normalized === "requesting" ? activeRequestStartedAtMs : null,
  };
}
