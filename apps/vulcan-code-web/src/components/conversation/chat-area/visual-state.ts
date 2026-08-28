import React from "react";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import {
  normalizeThinkingDisplayMode,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";

// CONVERSATION_ENTRY_MOTION_MS matches the empty-prompt and Composer entry transition duration.
// CONVERSATION_ENTRY_MOTION_MS 与空提示和输入区入场过渡时长保持一致。
const CONVERSATION_ENTRY_MOTION_MS = 360;

// FIRST_SUBMISSION_MASK_FADE_MS defines the visual-only mask fade after entry motion settles.
// FIRST_SUBMISSION_MASK_FADE_MS 定义入场动画稳定后纯视觉遮罩的淡出时长。
const FIRST_SUBMISSION_MASK_FADE_MS = 500;

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

// FirstSubmissionMaskPhase describes the visual-only first-submit mask lifecycle.
// FirstSubmissionMaskPhase 描述首次提交纯视觉遮罩的生命周期。
type FirstSubmissionMaskPhase = "hidden" | "holding" | "leaving";

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
  // Exact optimistic submission identity retained across draft materialization.
  // 在草稿物化过程中保留的精确乐观提交身份。
  firstSubmissionTransitionKey: string | null;
  // Whether the current blank frame is the same draft-to-conversation materialization.
  // 当前空白帧是否属于同一次草稿到会话物化过程。
  firstSubmissionMaterializing: boolean;
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
  // Whether the first-submit visual mask remains mounted.
  // 首次提交纯视觉遮罩是否仍保持挂载。
  firstSubmissionMaskVisible: boolean;
  // Whether the first-submit visual mask is fading out.
  // 首次提交纯视觉遮罩是否正在淡出。
  firstSubmissionMaskLeaving: boolean;
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
  // firstSubmissionMaskPhase retains a visual-only cover without gating transcript or Composer state.
  // firstSubmissionMaskPhase 保留纯视觉覆盖层，不阻塞转录或输入区状态。
  const [firstSubmissionMaskPhase, setFirstSubmissionMaskPhase] = React.useState<FirstSubmissionMaskPhase>("hidden");
  // previousSurfaceKeyRef detects actual surface changes before paint.
  // previousSurfaceKeyRef 在绘制前检测真实界面变化。
  const previousSurfaceKeyRef = React.useRef(options.surfaceKey);
  // previousEmptyConversationRef detects the one empty-to-materialized transition before paint.
  // previousEmptyConversationRef 在绘制前检测唯一的空白到物化过渡。
  const previousEmptyConversationRef = React.useRef(options.isEmptyConversation);
  // firstSubmissionMaskKeyRef owns the exact submission currently covered by the mask.
  // firstSubmissionMaskKeyRef 持有当前由遮罩覆盖的精确提交身份。
  const firstSubmissionMaskKeyRef = React.useRef<string | null>(null);
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
    // preserveFirstSubmissionMask allows only the exact draft promotion or its declared materialization frame.
    // preserveFirstSubmissionMask 仅允许精确草稿转正或其已声明物化帧继续保留遮罩。
    const preserveFirstSubmissionMask = firstSubmissionMaskKeyRef.current !== null
      && (
        firstSubmissionMaskKeyRef.current === options.firstSubmissionTransitionKey
        || options.firstSubmissionMaterializing
      );
    if (!preserveFirstSubmissionMask) {
      firstSubmissionMaskKeyRef.current = null;
      setFirstSubmissionMaskPhase("hidden");
    }
  }, [
    options.displayedThinkingBlock,
    options.firstSubmissionMaterializing,
    options.firstSubmissionTransitionKey,
    options.isEmptyConversation,
    options.surfaceKey,
  ]);

  React.useLayoutEffect(() => {
    // wasEmpty captures the committed visual state before the current render.
    // wasEmpty 捕获当前渲染前已经提交的视觉状态。
    const wasEmpty = previousEmptyConversationRef.current;
    previousEmptyConversationRef.current = options.isEmptyConversation;
    if (options.isEmptyConversation) {
      firstSubmissionMaskKeyRef.current = null;
      setFirstSubmissionMaskPhase("hidden");
      return;
    }
    if (!wasEmpty || !options.firstSubmissionTransitionKey) {
      return;
    }
    // The layout-phase commit guarantees the opaque mask exists before the first non-empty frame paints.
    // 布局阶段提交保证首个非空帧绘制前不透明遮罩已经存在。
    firstSubmissionMaskKeyRef.current = options.firstSubmissionTransitionKey;
    setFirstSubmissionMaskPhase("holding");
  }, [options.firstSubmissionTransitionKey, options.isEmptyConversation, options.surfaceKey]);

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
    }, CONVERSATION_ENTRY_MOTION_MS);
    return () => window.clearTimeout(exitTimer);
  }, [emptyPromptVisible, options.isEmptyConversation, options.surfaceKey]);

  React.useEffect(() => {
    if (firstSubmissionMaskPhase !== "holding") {
      return;
    }
    // revealTimer starts fading only after the existing content-entry motion has completed.
    // revealTimer 仅在现有内容入场动画完成后启动淡出。
    const revealTimer = window.setTimeout(() => {
      setFirstSubmissionMaskPhase("leaving");
    }, CONVERSATION_ENTRY_MOTION_MS);
    return () => window.clearTimeout(revealTimer);
  }, [firstSubmissionMaskPhase]);

  React.useEffect(() => {
    if (firstSubmissionMaskPhase !== "leaving") {
      return;
    }
    // removeTimer unmounts the non-interactive mask after its opacity transition.
    // removeTimer 在透明度过渡完成后卸载不可交互遮罩。
    const removeTimer = window.setTimeout(() => {
      firstSubmissionMaskKeyRef.current = null;
      setFirstSubmissionMaskPhase("hidden");
    }, FIRST_SUBMISSION_MASK_FADE_MS);
    return () => window.clearTimeout(removeTimer);
  }, [firstSubmissionMaskPhase]);

  return {
    visibleThinkingBlock,
    thinkingOverlayLeaving,
    emptyPromptVisible,
    emptyPromptLeaving,
    firstSubmissionMaskVisible: firstSubmissionMaskPhase !== "hidden",
    firstSubmissionMaskLeaving: firstSubmissionMaskPhase === "leaving",
  };
}
