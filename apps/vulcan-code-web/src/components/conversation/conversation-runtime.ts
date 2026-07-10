import type { useI18n } from "@/i18n";
import type { ConversationSummary } from "@/types";
import type {
  ConversationBlock,
  ConversationRenderItem,
  RuntimeCue,
} from "@/components/conversation/conversation-model";

// activeStreamingThinkingBlock returns the selected session thinking block that should float while streaming.
// activeStreamingThinkingBlock 返回当前选中会话在流式思考时应浮动展示的思考块。
export function activeStreamingThinkingBlock(
  conversation: ConversationSummary | undefined,
  renderItems: ConversationRenderItem[],
): ConversationBlock | null {
  if (normalizedSessionRunPhase(conversation?.runPhase) !== "thinking") {
    return null;
  }
  const blocks = renderItems
    .filter((item) => item.role !== "user")
    .flatMap((item) => item.blocks);
  return [...blocks].reverse().find((block) => block.kind === "thinking") ?? null;
}

// conversationRuntimeCue resolves the compact active-state cue shown above the composer.
// conversationRuntimeCue 解析显示在输入区上方的紧凑运行状态提示。
export function conversationRuntimeCue(
  conversation: ConversationSummary | undefined,
  _renderItems: ConversationRenderItem[],
  t: ReturnType<typeof useI18n>["t"],
): RuntimeCue | null {
  if (!conversation) {
    return null;
  }
  if (conversation.status.toLowerCase() !== "running") {
    return null;
  }
  const phaseCue = sessionRunPhaseCue(conversation.runPhase, t);
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
function sessionRunPhaseCue(
  runPhase: string | null | undefined,
  t: ReturnType<typeof useI18n>["t"],
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
  if (normalized === "toolrunning") {
    return null;
  }
  return { kind: "default", text: t("runtime.requesting") };
}
