import type { useI18n } from "@/i18n";
import type { ConversationSummary, WorkbenchSnapshot } from "@/types";
import { isMergeableToolBlock } from "@/components/conversation/conversation-classifiers";
import { toolExecutionDisplayState } from "@/components/conversation/conversation-tool-execution";
import type {
  ConversationBlock,
  ConversationRenderItem,
  RuntimeCue,
  StatusbarRuntimeActivity,
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

// buildStatusbarRuntimeActivities derives footer-only tool and subagent activities for the selected session.
// buildStatusbarRuntimeActivities 为当前选中会话推导仅显示在底部栏的工具与子代理活动。
export function buildStatusbarRuntimeActivities(
  conversation: ConversationSummary | undefined,
  projectedItems: ConversationRenderItem[],
  subagentNavigation: WorkbenchSnapshot["subagentNavigation"] | null | undefined,
  t: ReturnType<typeof useI18n>["t"],
): StatusbarRuntimeActivity[] {
  if (!conversation) {
    return [];
  }
  const normalized = normalizedSessionRunPhase(conversation.runPhase);
  const conversationRunning = conversationIsRunning(conversation);
  const activeToolItems = projectedItems.filter((item) => {
    const toolBlocks = item.blocks.filter(isMergeableToolBlock);
    return toolBlocks.length > 0 && toolExecutionDisplayState(toolBlocks, conversationRunning) === "running";
  });
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning) ?? false;
  if (!conversationRunning && !hasRunningSubagents && activeToolItems.length === 0) {
    return [];
  }
  const lastMeaningfulBlock = [...projectedItems]
    .reverse()
    .flatMap((item) => [...item.blocks].reverse())
    .find((block) => block.kind !== "turn-stats") ?? null;
  const hasActiveSubagentTool = activeToolItems.some((item) => item.blocks.some(blockLooksLikeSubagentTool));
  const hasActiveNormalTool = activeToolItems.some((item) => !item.blocks.some(blockLooksLikeSubagentTool));
  const phaseLooksLikeToolRunning = normalized === "toolrunning";
  const phaseLooksLikeSubagentTool = Boolean(lastMeaningfulBlock && blockLooksLikeSubagentTool(lastMeaningfulBlock));
  const activities: StatusbarRuntimeActivity[] = [];
  if (hasActiveNormalTool || (phaseLooksLikeToolRunning && !hasRunningSubagents && !hasActiveSubagentTool && !phaseLooksLikeSubagentTool)) {
    activities.push({ kind: "tool", text: t("runtime.toolActivity") });
  }
  if (hasRunningSubagents || hasActiveSubagentTool || (phaseLooksLikeToolRunning && phaseLooksLikeSubagentTool)) {
    activities.push({ kind: "subagent", text: t("runtime.subagentRunning") });
  }
  return activities;
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

// blockLooksLikeSubagentTool detects session child-agent coordination tools.
// blockLooksLikeSubagentTool 检测会话子代理协调工具。
function blockLooksLikeSubagentTool(block: ConversationBlock): boolean {
  const toolName = block.toolName?.trim().toLowerCase();
  return toolName === "spawn_agent"
    || toolName === "send_message"
    || toolName === "followup_task"
    || toolName === "wait_agent"
    || toolName === "list_agents"
    || toolName === "close_agent";
}
