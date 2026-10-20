import React from "react";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";


// THINKING_OVERLAY_EXIT_MS is how long the live reasoning window waits, then fades, once reasoning ends; its fade CSS reads it too.
// THINKING_OVERLAY_EXIT_MS 是思考结束后实时思考窗等待及淡出的时长；其淡出样式也读取该值。
export const THINKING_OVERLAY_EXIT_MS = 500;

// thinkingOverlayBlockSignature identifies one displayed thinking block without importing the legacy transcript parser.
// thinkingOverlayBlockSignature 在不导入旧转录解析器的前提下标识一个已显示的思考块。
function thinkingOverlayBlockSignature(block: ConversationBlock): string {
  return [
    block.id,
    block.title,
    block.summary,
    block.status ?? "",
  ].join("\u0000");
}


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

// UseConversationVisualTransitionsOptions defines one surface's empty and thinking-overlay transition inputs.
// UseConversationVisualTransitionsOptions 定义单个界面的空状态与思考浮层过渡输入。
export interface UseConversationVisualTransitionsOptions {
  // Stable conversation or draft surface key.
  // 稳定的会话或草稿界面键。
  surfaceKey: string;
  // Live thinking block that should currently occupy the floating overlay.
  // 当前应占用浮动面板的实时思考块。
  displayedThinkingBlock: ConversationBlock | null;
}

// ConversationVisualTransitionsState exposes retained visual state for ChatArea rendering.
// ConversationVisualTransitionsState 暴露 ChatArea 渲染使用的保留视觉状态。
export interface ConversationVisualTransitionsState {
  // Thinking block displayed live or retained during exit.
  // 实时展示或退场期间保留的思考块。
  visibleThinkingBlock: ConversationBlock | null;
  // Whether the retained thinking overlay is in its fade-out phase.
  // 保留的思考浮层是否处于淡出阶段。
  thinkingOverlayLeaving: boolean;
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
  // retainedThinkingBlock keeps the latest overlay mounted during its delayed exit.
  // retainedThinkingBlock 在延迟退场期间保持最新浮层挂载。
  const [retainedThinkingBlock, setRetainedThinkingBlock] = React.useState<ConversationBlock | null>(
    options.displayedThinkingBlock,
  );
  // thinkingOverlayLeaving marks the fade-out half of overlay exit.
  // thinkingOverlayLeaving 标记浮层退场的淡出阶段。
  const [thinkingOverlayLeaving, setThinkingOverlayLeaving] = React.useState(false);
  // previousSurfaceKeyRef detects actual surface changes before paint.
  // previousSurfaceKeyRef 在绘制前检测真实界面变化。
  const previousSurfaceKeyRef = React.useRef(options.surfaceKey);
  // displayedThinkingSignature identifies content changes inside one live overlay block.
  // displayedThinkingSignature 标识单个实时浮层块内部的内容变化。
  const displayedThinkingSignature = options.displayedThinkingBlock
    ? thinkingOverlayBlockSignature(options.displayedThinkingBlock)
    : null;
  // visibleThinkingBlock selects live content before delayed retained content.
  // visibleThinkingBlock 优先选择实时内容，再选择延迟保留内容。
  const visibleThinkingBlock = options.displayedThinkingBlock ?? retainedThinkingBlock;

  React.useLayoutEffect(() => {
    if (previousSurfaceKeyRef.current === options.surfaceKey) {
      return;
    }
    previousSurfaceKeyRef.current = options.surfaceKey;
    setRetainedThinkingBlock(options.displayedThinkingBlock);
    setThinkingOverlayLeaving(false);
  }, [
    options.displayedThinkingBlock,
    options.surfaceKey,
  ]);

  React.useEffect(() => {
    if (options.displayedThinkingBlock) {
      setRetainedThinkingBlock((current) => (
        current && thinkingOverlayBlockSignature(current) === displayedThinkingSignature
          ? current
          : options.displayedThinkingBlock
      ));
      setThinkingOverlayLeaving(false);
      return;
    }
    if (!retainedThinkingBlock) {
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
      setRetainedThinkingBlock(null);
      setThinkingOverlayLeaving(false);
    }, THINKING_OVERLAY_EXIT_MS * 2);
    return () => {
      window.clearTimeout(fadeTimer);
      window.clearTimeout(removeTimer);
    };
  }, [
    displayedThinkingSignature,
    options.displayedThinkingBlock,
    options.surfaceKey,
    retainedThinkingBlock,
  ]);

  return {
    visibleThinkingBlock,
    thinkingOverlayLeaving,
  };
}
