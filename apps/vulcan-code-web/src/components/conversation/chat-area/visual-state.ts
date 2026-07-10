import React from "react";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import {
  normalizeThinkingDisplayMode,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";
import { thinkingOverlayBlockSignature } from "@/components/conversation/conversation-transcript";

// UseThinkingDisplayModeOptions defines the authoritative preference and persistence action.
// UseThinkingDisplayModeOptions 定义权威偏好与持久化操作。
export interface UseThinkingDisplayModeOptions {
  // Optional backend-projected display mode.
  // 后端投影的可选展示模式。
  authoritativeMode: string | null | undefined;
  // Persists one user-selected display mode through the workbench service.
  // 通过工作台服务持久化一个用户选择的展示模式。
  onSelectMode: (mode: ThinkingDisplayMode) => Promise<boolean>;
}

// ThinkingDisplayModeState exposes the effective mode and guarded selection action.
// ThinkingDisplayModeState 暴露生效模式与受保护的选择操作。
export interface ThinkingDisplayModeState {
  // Effective mode used by transcript filtering and composer controls.
  // 转录过滤与输入区控件使用的生效模式。
  mode: ThinkingDisplayMode;
  // Applies one optimistic preference with stale-result-safe rollback.
  // 应用一次带过期结果安全回滚的乐观偏好。
  selectMode: (mode: ThinkingDisplayMode) => void;
}

// UseConversationVisualTransitionsOptions defines one surface's empty and thinking-overlay transition inputs.
// UseConversationVisualTransitionsOptions 定义单个界面的空状态与思考浮层过渡输入。
export interface UseConversationVisualTransitionsOptions {
  // Stable conversation or draft surface key.
  // 稳定的会话或草稿界面键。
  surfaceKey: string;
  // Whether the selected surface currently represents an empty conversation.
  // 当前选中界面是否表示空会话。
  isEmptyConversation: boolean;
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
  // Whether the empty-conversation prompt remains mounted.
  // 空会话提示是否仍保持挂载。
  emptyPromptVisible: boolean;
  // Whether the empty-conversation prompt is in its exit phase.
  // 空会话提示是否处于退场阶段。
  emptyPromptLeaving: boolean;
}

// useThinkingDisplayMode synchronizes a backend preference with optimistic user selections.
// useThinkingDisplayMode 将后端偏好与用户乐观选择保持同步。
// Parameters: options provides the authoritative value and persistence callback.
// 参数：options 提供权威值与持久化回调。
// Returns: the effective mode and a stale-result-safe selection action.
// 返回值：生效模式与过期结果安全的选择操作。
export function useThinkingDisplayMode(options: UseThinkingDisplayModeOptions): ThinkingDisplayModeState {
  // authoritativeMode is the normalized current backend preference.
  // authoritativeMode 是规范化后的当前后端偏好。
  const authoritativeMode = normalizeThinkingDisplayMode(options.authoritativeMode);
  // mode stores the optimistic preference rendered by the current workbench.
  // mode 保存当前工作台渲染的乐观偏好。
  const [mode, setMode] = React.useState<ThinkingDisplayMode>(authoritativeMode);
  // authoritativeModeRef exposes the latest backend value to retained async callbacks.
  // authoritativeModeRef 向保留的异步回调暴露最新后端值。
  const authoritativeModeRef = React.useRef(authoritativeMode);
  authoritativeModeRef.current = authoritativeMode;
  // pendingModeRef identifies the latest optimistic value awaiting persistence.
  // pendingModeRef 标识等待持久化的最新乐观值。
  const pendingModeRef = React.useRef<ThinkingDisplayMode | null>(null);
  // requestVersionRef prevents an older completion from rolling back a newer selection.
  // requestVersionRef 防止较早完成结果回滚较新的选择。
  const requestVersionRef = React.useRef(0);

  React.useEffect(() => {
    // pendingMode is the optimistic selection that may intentionally differ from an older snapshot.
    // pendingMode 是可能有意区别于旧快照的乐观选择。
    const pendingMode = pendingModeRef.current;
    if (pendingMode && pendingMode !== authoritativeMode) {
      return;
    }
    if (pendingMode === authoritativeMode) {
      pendingModeRef.current = null;
    }
    setMode(authoritativeMode);
  }, [authoritativeMode]);

  // selectMode applies one optimistic value and lets only the latest failed request roll it back.
  // selectMode 应用一个乐观值，仅允许最新失败请求执行回滚。
  const selectMode = React.useCallback((nextMode: ThinkingDisplayMode) => {
    // requestVersion identifies this preference request among overlapping selections.
    // requestVersion 在重叠选择中标识本次偏好请求。
    const requestVersion = requestVersionRef.current + 1;
    requestVersionRef.current = requestVersion;
    pendingModeRef.current = nextMode;
    setMode(nextMode);
    void options.onSelectMode(nextMode).then((saved) => {
      if (requestVersionRef.current !== requestVersion) {
        return;
      }
      pendingModeRef.current = null;
      if (!saved) {
        setMode(authoritativeModeRef.current);
      }
    });
  }, [options.onSelectMode]);

  return { mode, selectMode };
}

// useConversationVisualTransitions owns scope-safe thinking-overlay and empty-prompt exit retention.
// useConversationVisualTransitions 管理作用域安全的思考浮层与空提示退场保留。
// Parameters: options provides the active surface, empty state, and live overlay block.
// 参数：options 提供活动界面、空状态与实时浮层块。
// Returns: retained overlay and empty-prompt visibility/exit state.
// 返回值：保留浮层与空提示的可见性/退场状态。
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
  // emptyPromptVisible keeps the empty prompt mounted during its exit animation.
  // emptyPromptVisible 在退场动画期间保持空提示挂载。
  const [emptyPromptVisible, setEmptyPromptVisible] = React.useState(options.isEmptyConversation);
  // emptyPromptLeaving marks the empty prompt exit animation.
  // emptyPromptLeaving 标记空提示退场动画。
  const [emptyPromptLeaving, setEmptyPromptLeaving] = React.useState(false);
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
    setEmptyPromptVisible(options.isEmptyConversation);
    setEmptyPromptLeaving(false);
  }, [options.displayedThinkingBlock, options.isEmptyConversation, options.surfaceKey]);

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
    }, 500);
    // removeTimer unmounts the retained overlay after the complete exit interval.
    // removeTimer 在完整退场间隔后卸载保留浮层。
    const removeTimer = window.setTimeout(() => {
      setRetainedThinkingBlock(null);
      setThinkingOverlayLeaving(false);
    }, 1000);
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

  React.useEffect(() => {
    if (options.isEmptyConversation) {
      setEmptyPromptVisible(true);
      setEmptyPromptLeaving(false);
      return;
    }
    if (!emptyPromptVisible) {
      return;
    }
    setEmptyPromptLeaving(true);
    // exitTimer removes the empty prompt after its CSS transition interval.
    // exitTimer 在 CSS 过渡间隔后移除空提示。
    const exitTimer = window.setTimeout(() => {
      setEmptyPromptVisible(false);
      setEmptyPromptLeaving(false);
    }, 360);
    return () => window.clearTimeout(exitTimer);
  }, [emptyPromptVisible, options.isEmptyConversation, options.surfaceKey]);

  return {
    visibleThinkingBlock,
    thinkingOverlayLeaving,
    emptyPromptVisible,
    emptyPromptLeaving,
  };
}
