import * as React from "react";

import type { ConversationMeasuredCollapseMemory } from "@/components/conversation/conversation-layout-restore";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { cn } from "@/lib/utils";

// CONVERSATION_MEASURED_COLLAPSE_MS matches the transcript fold animation duration.
// CONVERSATION_MEASURED_COLLAPSE_MS 对齐会话折叠动画时长。
const CONVERSATION_MEASURED_COLLAPSE_MS = 180;

// ConversationMeasuredCollapseMap stores collapse memory by stable collapse id.
// ConversationMeasuredCollapseMap 按稳定折叠标识保存折叠记忆。
export type ConversationMeasuredCollapseMap = Record<string, ConversationMeasuredCollapseMemory>;

// ConversationMeasuredCollapseContextValue exposes session-scoped collapse memory.
// ConversationMeasuredCollapseContextValue 暴露会话作用域内的折叠记忆。
interface ConversationMeasuredCollapseContextValue {
  // Returns the current memory for one collapse id.
  // 返回单个折叠标识的当前记忆。
  getMemory: (collapseId: string) => ConversationMeasuredCollapseMemory | null;
  // Updates the expansion state for one collapse id.
  // 更新单个折叠标识的展开状态。
  setExpanded: (collapseId: string, expanded: boolean) => void;
  // Updates stable metrics for one collapse id.
  // 更新单个折叠标识的稳定测量数据。
  setMetrics: (collapseId: string, metrics: Partial<ConversationMeasuredCollapseMemory>) => void;
}

// ConversationMeasuredCollapseContext stores per-surface measured collapse state.
// ConversationMeasuredCollapseContext 保存单界面的折叠测量状态。
const ConversationMeasuredCollapseContext = React.createContext<ConversationMeasuredCollapseContextValue | null>(null);

// ConversationMeasuredCollapseProviderProps configures one transcript collapse-memory scope.
// ConversationMeasuredCollapseProviderProps 配置单个转录界面的折叠记忆作用域。
export interface ConversationMeasuredCollapseProviderProps {
  // Initial collapse memory restored for the active transcript surface.
  // 为当前转录界面恢复的初始折叠记忆。
  initialMemory: ConversationMeasuredCollapseMap;
  // Stable scope id used to reset memory when switching surfaces.
  // 切换界面时用于重置记忆的稳定作用域标识。
  scopeId: string | null;
  // Persist callback invoked when collapse memory changes.
  // 折叠记忆变化时调用的持久化回调。
  onMemoryChange: (memory: ConversationMeasuredCollapseMap) => void;
  // Descendant transcript content.
  // 下级转录内容。
  children: React.ReactNode;
}

// ConversationMeasuredCollapseProvider owns measured collapse state for one transcript surface.
// ConversationMeasuredCollapseProvider 持有单个转录界面的折叠测量状态。
export function ConversationMeasuredCollapseProvider(props: ConversationMeasuredCollapseProviderProps) {
  const { children, initialMemory, onMemoryChange, scopeId } = props;
  const [memory, setMemory] = React.useState<ConversationMeasuredCollapseMap>(() => ({ ...initialMemory }));
  const pendingNotificationRef = React.useRef<ConversationMeasuredCollapseMap | null>(null);
  const scopeIdRef = React.useRef(scopeId);

  React.useEffect(() => {
    if (scopeIdRef.current !== scopeId) {
      scopeIdRef.current = scopeId;
      pendingNotificationRef.current = null;
      setMemory({ ...initialMemory });
    }
  }, [initialMemory, scopeId]);

  React.useEffect(() => {
    if (pendingNotificationRef.current !== memory) {
      return;
    }
    pendingNotificationRef.current = null;
    onMemoryChange(memory);
  }, [memory, onMemoryChange]);

  const updateMemory = React.useCallback((updater: (current: ConversationMeasuredCollapseMap) => ConversationMeasuredCollapseMap) => {
    setMemory((current) => {
      const next = updater(current);
      if (next === current) {
        return current;
      }
      pendingNotificationRef.current = next;
      return next;
    });
  }, []);

  const getMemory = React.useCallback((collapseId: string) => memory[collapseId] ?? null, [memory]);
  const setExpanded = React.useCallback((collapseId: string, expanded: boolean) => {
    updateMemory((current) => {
      const previous = current[collapseId] ?? {};
      if (previous.expanded === expanded) {
        return current;
      }
      return {
        ...current,
        [collapseId]: {
          ...previous,
          expanded,
        },
      };
    });
  }, [updateMemory]);
  const setMetrics = React.useCallback((collapseId: string, metrics: Partial<ConversationMeasuredCollapseMemory>) => {
    updateMemory((current) => {
      const previous = current[collapseId] ?? {};
      const nextMemory = {
        ...previous,
        ...metrics,
      };
      if (conversationMeasuredCollapseMemoryEqual(previous, nextMemory)) {
        return current;
      }
      return {
        ...current,
        [collapseId]: nextMemory,
      };
    });
  }, [updateMemory]);
  const value = React.useMemo<ConversationMeasuredCollapseContextValue>(() => ({
    getMemory,
    setExpanded,
    setMetrics,
  }), [getMemory, setExpanded, setMetrics]);

  return (
    <ConversationMeasuredCollapseContext.Provider value={value}>
      {children}
    </ConversationMeasuredCollapseContext.Provider>
  );
}

// useConversationMeasuredExpansion returns a unified expansion state for one collapse id.
// useConversationMeasuredExpansion 返回单个折叠标识的统一展开状态。
export function useConversationMeasuredExpansion(
  // Stable collapse id.
  // 稳定折叠标识。
  collapseId: string,
  // Fallback state used when no memory exists.
  // 没有记忆时使用的兜底状态。
  fallbackExpanded: boolean,
): [boolean, (expanded: boolean, options?: { manual?: boolean }) => void] {
  const context = React.useContext(ConversationMeasuredCollapseContext);
  const remembered = context?.getMemory(collapseId)?.expanded;
  const contextSetExpanded = context?.setExpanded;
  const [localExpanded, setLocalExpanded] = React.useState(remembered ?? fallbackExpanded);

  React.useEffect(() => {
    setLocalExpanded(remembered ?? fallbackExpanded);
  }, [collapseId, fallbackExpanded, remembered]);

  const setExpanded = React.useCallback((expanded: boolean, options?: { manual?: boolean }) => {
    if (options?.manual !== false) {
      dispatchConversationManualViewportInteraction();
    }
    contextSetExpanded?.(collapseId, expanded);
    setLocalExpanded(expanded);
  }, [collapseId, contextSetExpanded]);

  return [localExpanded, setExpanded];
}

// ConversationMeasuredCollapseProps describes one virtual-scroll-aware collapse shell.
// ConversationMeasuredCollapseProps 描述单个可被虚拟滚动感知的折叠外壳。
export interface ConversationMeasuredCollapseProps {
  // Stable collapse id used for expansion and height memory.
  // 用于展开状态与高度记忆的稳定折叠标识。
  collapseId: string;
  // Whether the content should be visible after the current transition.
  // 当前过渡结束后内容是否应该可见。
  expanded: boolean;
  // Optional non-zero preview height kept while the content is collapsed.
  // 内容收起时保留的可选非零预览高度。
  collapsedHeight?: number;
  // Optional class name applied to the animated outer shell.
  // 应用于动画外层容器的可选类名。
  className?: string;
  // Folded transcript content rendered while open or closing.
  // 打开或收起动画期间渲染的折叠内容。
  children: React.ReactNode;
}

// ConversationMeasuredCollapse animates fold boxes while exposing stable target heights for virtualization.
// ConversationMeasuredCollapse 为折叠框提供动画，并向虚拟滚动暴露稳定目标高度。
export function ConversationMeasuredCollapse(props: ConversationMeasuredCollapseProps) {
  const context = React.useContext(ConversationMeasuredCollapseContext);
  const contextSetMetrics = context?.setMetrics;
  const prefersReducedMotion = usePrefersReducedMotion();
  const collapsedHeight = Math.max(0, props.collapsedHeight ?? 0);
  const [rendered, setRendered] = React.useState(props.expanded || collapsedHeight > 0);
  const [visualOpen, setVisualOpen] = React.useState(props.expanded);
  const [height, setHeight] = React.useState(props.expanded ? "auto" : `${collapsedHeight}px`);
  const [targetHeight, setTargetHeight] = React.useState<number | null>(props.expanded ? null : collapsedHeight);
  const innerRef = React.useRef<HTMLDivElement | null>(null);
  const expandedRef = React.useRef(props.expanded);
  const heightRef = React.useRef(height);
  const previousExpandedRef = React.useRef(props.expanded);
  const frameRef = React.useRef<number | null>(null);
  const fallbackTimerRef = React.useRef<number | null>(null);
  // collapsingRef prevents stable collapsed-state synchronization from unmounting content before the closing height transition finishes.
  // collapsingRef 防止稳定折叠状态同步在收起高度动画完成前卸载内容。
  const collapsingRef = React.useRef(false);

  React.useEffect(() => {
    expandedRef.current = props.expanded;
  }, [props.expanded]);

  React.useEffect(() => {
    heightRef.current = height;
  }, [height]);

  React.useEffect(() => {
    if (props.expanded || collapsedHeight > 0) {
      setRendered(true);
    }
  }, [collapsedHeight, props.expanded]);

  const clearScheduledWork = React.useCallback(() => {
    if (frameRef.current !== null) {
      window.cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    if (fallbackTimerRef.current !== null) {
      window.clearTimeout(fallbackTimerRef.current);
      fallbackTimerRef.current = null;
    }
  }, []);

  const measureExpandedHeight = React.useCallback((): number => {
    return innerRef.current?.scrollHeight ?? 0;
  }, []);

  const rememberMetrics = React.useCallback((metrics: Partial<ConversationMeasuredCollapseMemory>) => {
    contextSetMetrics?.(props.collapseId, metrics);
  }, [contextSetMetrics, props.collapseId]);

  const finishTransition = React.useCallback(() => {
    fallbackTimerRef.current = null;
    collapsingRef.current = false;
    if (expandedRef.current) {
      const expandedHeight = measureExpandedHeight();
      rememberMetrics({
        expanded: true,
        expandedHeight,
        stableHeight: expandedHeight,
      });
      setTargetHeight(null);
      setVisualOpen(true);
      setHeight("auto");
      return;
    }
    rememberMetrics({
      collapsedHeight,
      expanded: false,
      stableHeight: collapsedHeight,
    });
    setTargetHeight(collapsedHeight);
    setVisualOpen(false);
    setHeight(`${collapsedHeight}px`);
    setRendered(collapsedHeight > 0);
  }, [collapsedHeight, measureExpandedHeight, rememberMetrics]);

  const scheduleTransitionFinish = React.useCallback(() => {
    if (fallbackTimerRef.current !== null) {
      window.clearTimeout(fallbackTimerRef.current);
    }
    fallbackTimerRef.current = window.setTimeout(finishTransition, CONVERSATION_MEASURED_COLLAPSE_MS + 80);
  }, [finishTransition]);

  React.useLayoutEffect(() => {
    const previousExpanded = previousExpandedRef.current;
    // State updates made during a closing transition must not restart and cancel its scheduled animation frame or completion timer.
    // 收起过渡中的状态更新不得重启并取消其已调度的动画帧或完成计时器。
    if (collapsingRef.current && !props.expanded) {
      return;
    }
    if (previousExpanded === props.expanded && rendered === (props.expanded || collapsedHeight > 0)) {
      return;
    }
    previousExpandedRef.current = props.expanded;
    clearScheduledWork();

    if (prefersReducedMotion) {
      collapsingRef.current = false;
      const stableHeight = props.expanded ? measureExpandedHeight() : collapsedHeight;
      rememberMetrics({
        collapsedHeight: props.expanded ? undefined : collapsedHeight,
        expanded: props.expanded,
        expandedHeight: props.expanded ? stableHeight : undefined,
        stableHeight,
      });
      setRendered(props.expanded || collapsedHeight > 0);
      setVisualOpen(props.expanded);
      setTargetHeight(props.expanded ? null : collapsedHeight);
      setHeight(props.expanded ? "auto" : `${collapsedHeight}px`);
      return;
    }

    if (props.expanded) {
      collapsingRef.current = false;
      setRendered(true);
      setVisualOpen(false);
      setTargetHeight(collapsedHeight);
      setHeight(`${collapsedHeight}px`);
      frameRef.current = window.requestAnimationFrame(() => {
        frameRef.current = null;
        const nextHeight = measureExpandedHeight();
        setTargetHeight(nextHeight);
        rememberMetrics({
          expanded: true,
          expandedHeight: nextHeight,
          stableHeight: nextHeight,
        });
        setVisualOpen(true);
        setHeight(`${nextHeight}px`);
        scheduleTransitionFinish();
      });
      return;
    }

    if (!rendered) {
      return;
    }

    const currentHeight = measureExpandedHeight();
    collapsingRef.current = true;
    setTargetHeight(collapsedHeight);
    rememberMetrics({
      collapsedHeight,
      expanded: false,
      stableHeight: collapsedHeight,
    });
    setVisualOpen(true);
    setHeight(`${currentHeight}px`);
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      setVisualOpen(false);
      setHeight(`${collapsedHeight}px`);
      scheduleTransitionFinish();
    });
  }, [clearScheduledWork, collapsedHeight, measureExpandedHeight, prefersReducedMotion, props.expanded, rendered, rememberMetrics, scheduleTransitionFinish]);

  React.useLayoutEffect(() => {
    // The primary transition effect owns the closing phase; this effect only synchronizes a stable collapsed height.
    // 主过渡 effect 负责收起阶段；此 effect 仅同步稳定折叠高度。
    if (props.expanded || collapsingRef.current) {
      return;
    }
    rememberMetrics({
      collapsedHeight,
      expanded: false,
      stableHeight: collapsedHeight,
    });
    setTargetHeight(collapsedHeight);
    setHeight(`${collapsedHeight}px`);
    setRendered(collapsedHeight > 0);
  }, [collapsedHeight, props.expanded, rememberMetrics]);

  React.useLayoutEffect(() => {
    if (!rendered || typeof ResizeObserver === "undefined") {
      return;
    }
    const element = innerRef.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(() => {
      if (expandedRef.current) {
        const nextHeight = element.scrollHeight;
        rememberMetrics({
          expanded: true,
          expandedHeight: nextHeight,
          stableHeight: nextHeight,
        });
        if (heightRef.current !== "auto") {
          setTargetHeight(nextHeight);
          setHeight(`${nextHeight}px`);
        }
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [rememberMetrics, rendered]);

  React.useEffect(() => clearScheduledWork, [clearScheduledWork]);

  if (!rendered) {
    return null;
  }

  return (
    <div
      aria-hidden={!props.expanded && collapsedHeight <= 0}
      className={cn("conversation-collapsible-motion", visualOpen && "open", props.className)}
      data-conversation-measured-collapse-id={props.collapseId}
      data-conversation-measured-target-height={targetHeight === null ? undefined : String(targetHeight)}
      onTransitionEnd={(event) => {
        if (event.currentTarget !== event.target || event.propertyName !== "height") {
          return;
        }
        if (fallbackTimerRef.current !== null) {
          window.clearTimeout(fallbackTimerRef.current);
          fallbackTimerRef.current = null;
        }
        finishTransition();
      }}
      style={{ height }}
    >
      <div className="conversation-collapsible-motion-inner" ref={innerRef}>
        {props.children}
      </div>
    </div>
  );
}

// usePrefersReducedMotion mirrors the browser motion preference for transcript animations.
// usePrefersReducedMotion 同步浏览器的动效偏好用于会话动画。
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = React.useState(false);
  React.useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(query.matches);
    const handleChange = () => setReduced(query.matches);
    query.addEventListener("change", handleChange);
    return () => query.removeEventListener("change", handleChange);
  }, []);
  return reduced;
}

// conversationMeasuredCollapseMapEqual compares collapse maps without treating object identity as a change.
// conversationMeasuredCollapseMapEqual 比较折叠映射，避免把对象引用变化误判为内容变化。
export function conversationMeasuredCollapseMapEqual(
  left: ConversationMeasuredCollapseMap,
  right: ConversationMeasuredCollapseMap,
): boolean {
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  if (leftKeys.length !== rightKeys.length) {
    return false;
  }
  return leftKeys.every((key) => conversationMeasuredCollapseMemoryEqual(left[key] ?? {}, right[key] ?? {}));
}

// conversationMeasuredCollapseMemoryEqual compares the persisted fields for one collapse node.
// conversationMeasuredCollapseMemoryEqual 比较单个折叠节点的持久化字段。
function conversationMeasuredCollapseMemoryEqual(
  left: ConversationMeasuredCollapseMemory,
  right: ConversationMeasuredCollapseMemory,
): boolean {
  return left.collapsedHeight === right.collapsedHeight
    && left.expanded === right.expanded
    && left.expandedHeight === right.expandedHeight
    && left.stableHeight === right.stableHeight;
}
