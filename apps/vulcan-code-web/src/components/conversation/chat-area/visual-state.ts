import React from "react";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";


// THINKING_OVERLAY_EXIT_MS is how long the live reasoning window waits, then fades, once reasoning ends; its fade CSS reads it too.
// THINKING_OVERLAY_EXIT_MS 是思考结束后实时思考窗等待及淡出的时长；其淡出样式也读取该值。
export const THINKING_OVERLAY_EXIT_MS = 500;

// UseThinkingDisplayModeOptions defines the global default and exact session scope.
// UseThinkingDisplayModeOptions 定义全局默认值及精确会话作用域。
export interface UseThinkingDisplayModeOptions {
  // Optional backend-projected display mode.
  // 后端投影的可选展示模式。
  authoritativeMode: string | null | undefined;
  // Exact conversation or draft whose local display choice is resolved.
  // 需要解析本地展示选择的精确会话或草稿。
  conversationId: string | null;
}

// ThinkingDisplayModeState exposes the effective mode and guarded selection action.
// ThinkingDisplayModeState 暴露生效模式与受保护的选择操作。
export interface ThinkingDisplayModeState {
  // Effective mode used by transcript filtering and composer controls.
  // 转录过滤与输入区控件使用的生效模式。
  mode: ThinkingDisplayMode;
  // Saves only the current session's explicit display preference.
  // 仅保存当前会话的明确展示偏好。
  selectMode: (mode: ThinkingDisplayMode) => void;
}

// UseConversationVisualTransitionsOptions defines the thinking overlay's inputs and conversation scope.
// UseConversationVisualTransitionsOptions 定义思考浮层的输入及会话作用域。
export interface UseConversationVisualTransitionsOptions {
  // Stable conversation or draft surface key.
  // 稳定的会话或草稿界面键。
  surfaceKey: string;
  // Live thinking block that should currently occupy the floating overlay.
  // 当前应占用浮动面板的实时思考块。
  displayedThinkingBlock: ConversationBlock | null;
}

// ConversationVisualTransitionsState exposes retained visual state to the thinking overlay.
// ConversationVisualTransitionsState 向思考浮层提供退场期间保留的视觉状态。
export interface ConversationVisualTransitionsState {
  // Thinking block displayed live or retained during exit.
  // 实时展示或退场期间保留的思考块。
  visibleThinkingBlock: ConversationBlock | null;
  // Whether the retained thinking overlay is in its fade-out phase.
  // 保留的思考浮层是否处于淡出阶段。
  thinkingOverlayLeaving: boolean;
}

// ScopedThinkingBlock binds an overlay snapshot to the surface that can display it.
// ScopedThinkingBlock 将浮层快照绑定到可显示它的界面。
interface ScopedThinkingBlock {
  // Surface that owns the snapshot.
  // 持有快照的界面。
  surfaceKey: string;
  // Last committed live block at the time of capture.
  // 捕获时最近一次提交的实时思考块。
  block: ConversationBlock;
}

// Resolves a session override without rewriting the global service preference.
// 解析会话覆盖值，不改写全局服务偏好。
// Parameters: options provides the authoritative global default and exact conversation ID.
// 参数：options 提供权威全局默认值及精确会话标识。
// Returns the effective mode and session-scoped selection action.
// 返回生效模式及会话作用域内的选择操作。
export function useThinkingDisplayMode(options: UseThinkingDisplayModeOptions): ThinkingDisplayModeState {
  return useThinkingDisplayPreference(options.conversationId, options.authoritativeMode);
}

// useConversationVisualTransitions owns scope-safe thinking-overlay exit retention.
// useConversationVisualTransitions 管理作用域安全的思考浮层退场保留。
// Parameters: options provides the active surface and live overlay block.
// 参数：options 提供活动界面与实时浮层块。
// Returns: retained overlay state.
// 返回值：保留浮层状态。
export function useConversationVisualTransitions(
  options: UseConversationVisualTransitionsOptions,
): ConversationVisualTransitionsState {
  // surfaceKey and displayedThinkingBlock are the current owner and live content.
  // surfaceKey 与 displayedThinkingBlock 分别是当前归属与实时内容。
  const { surfaceKey, displayedThinkingBlock } = options;
  // hasLiveThinking changes only when the live overlay opens or closes.
  // hasLiveThinking 仅在实时浮层出现或退出时变化。
  const hasLiveThinking = displayedThinkingBlock !== null;
  // committedThinkingBlock tracks revisions without scheduling another conversation render.
  // committedThinkingBlock 记录各次修订，无需再次渲染会话。
  const committedThinkingBlock = React.useRef<ScopedThinkingBlock | null>(null);
  // retainedThinkingBlock keeps the overlay mounted while it transitions to its final snapshot.
  // retainedThinkingBlock 让浮层在切换到最终快照期间保持挂载。
  const [retainedThinkingBlock, setRetainedThinkingBlock] = React.useState<ScopedThinkingBlock | null>(null);
  // thinkingOverlayLeaving marks the fade-out half of overlay exit.
  // thinkingOverlayLeaving 标记浮层退场的淡出阶段。
  const [thinkingOverlayLeaving, setThinkingOverlayLeaving] = React.useState(false);

  React.useLayoutEffect(() => {
    if (displayedThinkingBlock) {
      committedThinkingBlock.current = { surfaceKey, block: displayedThinkingBlock };
    }
  }, [displayedThinkingBlock, surfaceKey]);

  React.useLayoutEffect(() => {
    // latest is the most recent block that actually reached the UI.
    // latest 是最近一次实际提交到界面的思考块。
    const latest = committedThinkingBlock.current;
    // scopedBlock can only survive within its originating conversation or draft.
    // scopedBlock 只能在其所属的会话或草稿内保留。
    const scopedBlock = latest?.surfaceKey === surfaceKey ? latest : null;
    committedThinkingBlock.current = scopedBlock;
    setRetainedThinkingBlock(scopedBlock);
    setThinkingOverlayLeaving(false);
    if (hasLiveThinking || !scopedBlock) {
      return;
    }
    // fadeTimer starts the visual exit after the live overlay disappears.
    // fadeTimer 在实时浮层消失后启动视觉退场。
    const fadeTimer = window.setTimeout(() => {
      setThinkingOverlayLeaving(true);
    }, THINKING_OVERLAY_EXIT_MS);
    // removeTimer unmounts the retained overlay after the complete exit interval.
    // removeTimer 在完整退场间隔后卸载保留浮层。
    const removeTimer = window.setTimeout(() => {
      committedThinkingBlock.current = null;
      setRetainedThinkingBlock(null);
      setThinkingOverlayLeaving(false);
    }, THINKING_OVERLAY_EXIT_MS * 2);
    return () => {
      window.clearTimeout(fadeTimer);
      window.clearTimeout(removeTimer);
    };
  }, [hasLiveThinking, surfaceKey]);

  // retainedBlock keeps the outgoing overlay in place only while its surface still owns it.
  // retainedBlock 仅在当前界面仍拥有浮层时保留退场内容。
  const retainedBlock = retainedThinkingBlock?.surfaceKey === surfaceKey ? retainedThinkingBlock.block : null;
  return {
    visibleThinkingBlock: displayedThinkingBlock ?? retainedBlock,
    thinkingOverlayLeaving: !hasLiveThinking && retainedBlock !== null && thinkingOverlayLeaving,
  };
}
