import * as React from "react";

import type { ConversationMeasuredCollapseMemory } from "@/components/conversation/conversation-layout-restore";
import {
  dispatchConversationLayoutSettled,
  dispatchConversationManualViewportInteraction,
} from "@/components/conversation/conversation-layout-events";
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
  // Subscribes one collapse id without invalidating unrelated consumers.
  // 订阅单个折叠标识，且不会使无关消费者失效。
  subscribe: (collapseId: string, listener: () => void) => () => void;
  // Updates the expansion state for one collapse id.
  // 更新单个折叠标识的展开状态。
  setExpanded: (collapseId: string, expanded: boolean) => void;
}

// ConversationMeasuredCollapseContext stores per-surface collapse expansion memory.
// ConversationMeasuredCollapseContext 保存单界面的折叠展开记忆。
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

// ConversationMeasuredCollapseProvider owns collapse expansion memory for one transcript surface.
// ConversationMeasuredCollapseProvider 持有单个转录界面的折叠展开记忆。
export function ConversationMeasuredCollapseProvider(props: ConversationMeasuredCollapseProviderProps) {
  const { children, initialMemory, onMemoryChange, scopeId } = props;
  // memoryRef owns the complete scope without using Provider state as a broadcast channel.
  // memoryRef 持有完整作用域，且不会把 Provider 状态当作广播通道。
  const memoryRef = React.useRef<ConversationMeasuredCollapseMap>({ ...initialMemory });
  // listenersRef isolates notifications by stable collapse id.
  // listenersRef 按稳定折叠标识隔离通知。
  const listenersRef = React.useRef(new Map<string, Set<() => void>>());
  const pendingNotificationRef = React.useRef<ConversationMeasuredCollapseMap | null>(null);
  // notificationQueuedRef coalesces persistence callbacks across one synchronous interaction.
  // notificationQueuedRef 在一次同步交互内合并持久化回调。
  const notificationQueuedRef = React.useRef(false);
  // mountedRef prevents a queued persistence callback after Provider unmount.
  // mountedRef 防止 Provider 卸载后执行已排队的持久化回调。
  const mountedRef = React.useRef(true);
  // onMemoryChangeRef keeps the stable external Store bound to the latest parent callback.
  // onMemoryChangeRef 让稳定外部 Store 始终绑定最新父级回调。
  const onMemoryChangeRef = React.useRef(onMemoryChange);
  onMemoryChangeRef.current = onMemoryChange;
  const scopeIdRef = React.useRef(scopeId);

  React.useLayoutEffect(() => {
    if (scopeIdRef.current !== scopeId) {
      // previousMemory and nextMemory determine the exact ids whose snapshots changed at cutover.
      // previousMemory 与 nextMemory 确定切换时快照变化的精确标识。
      const previousMemory = memoryRef.current;
      const nextMemory = { ...initialMemory };
      scopeIdRef.current = scopeId;
      pendingNotificationRef.current = null;
      memoryRef.current = nextMemory;
      const changedIds = new Set([
        ...Object.keys(previousMemory),
        ...Object.keys(nextMemory),
      ]);
      for (const collapseId of changedIds) {
        if (previousMemory[collapseId]?.expanded === nextMemory[collapseId]?.expanded) {
          continue;
        }
        for (const listener of listenersRef.current.get(collapseId) ?? []) {
          listener();
        }
      }
    }
  }, [initialMemory, scopeId]);

  React.useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      pendingNotificationRef.current = null;
    };
  }, []);

  const getMemory = React.useCallback(
    (collapseId: string) => memoryRef.current[collapseId] ?? null,
    [],
  );
  const subscribe = React.useCallback((collapseId: string, listener: () => void) => {
    // listeners contains only consumers of the exact collapse identity.
    // listeners 仅包含当前精确折叠身份的消费者。
    const listeners = listenersRef.current.get(collapseId) ?? new Set<() => void>();
    listeners.add(listener);
    listenersRef.current.set(collapseId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        listenersRef.current.delete(collapseId);
      }
    };
  }, []);
  const setExpanded = React.useCallback((collapseId: string, expanded: boolean) => {
    // current is the exact immutable memory snapshot before this leaf update.
    // current 是当前叶子更新前的精确不可变记忆快照。
    const current = memoryRef.current;
    if (current[collapseId]?.expanded === expanded) {
      return;
    }
    // next replaces only the selected collapse identity.
    // next 仅替换选中的折叠身份。
    const next = {
      ...current,
      [collapseId]: { expanded },
    };
    memoryRef.current = next;
    pendingNotificationRef.current = next;
    for (const listener of listenersRef.current.get(collapseId) ?? []) {
      listener();
    }
    if (notificationQueuedRef.current) {
      return;
    }
    notificationQueuedRef.current = true;
    queueMicrotask(() => {
      notificationQueuedRef.current = false;
      // pending is the newest immutable memory produced during this interaction.
      // pending 是当前交互期间生成的最新不可变记忆。
      const pending = pendingNotificationRef.current;
      pendingNotificationRef.current = null;
      if (mountedRef.current && pending) {
        onMemoryChangeRef.current(pending);
      }
    });
  }, []);
  const value = React.useMemo<ConversationMeasuredCollapseContextValue>(() => ({
    getMemory,
    subscribe,
    setExpanded,
  }), [getMemory, setExpanded, subscribe]);

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
  // subscribe binds React only to this exact collapse identity.
  // subscribe 仅将 React 绑定到当前精确折叠身份。
  const subscribe = React.useCallback(
    (listener: () => void) => context?.subscribe(collapseId, listener) ?? (() => {}),
    [collapseId, context],
  );
  // getSnapshot returns a primitive expansion value with stable equality semantics.
  // getSnapshot 返回具有稳定相等语义的原始展开值。
  const getSnapshot = React.useCallback(
    () => context?.getMemory(collapseId)?.expanded,
    [collapseId, context],
  );
  const remembered = React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
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

// ConversationMeasuredCollapseProps describes one native-flow animated collapse shell.
// ConversationMeasuredCollapseProps 描述单个原生流动画折叠外壳。
export interface ConversationMeasuredCollapseProps {
  // Stable collapse id used for remount-safe expansion memory.
  // 用于可安全重新挂载展开记忆的稳定折叠标识。
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

// ConversationMeasuredCollapse animates fold boxes from current committed DOM measurements.
// ConversationMeasuredCollapse 使用当前已提交 DOM 测量为折叠框提供动画。
export function ConversationMeasuredCollapse(props: ConversationMeasuredCollapseProps) {
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
  // transitioningRef owns the exact interval between a geometry target change and stable commit.
  // transitioningRef 持有从几何目标变化到稳定提交之间的精确区间。
  const transitioningRef = React.useRef(false);
  // transitionStartedAtRef rejects transitionend events from superseded geometry targets.
  // transitionStartedAtRef 拒绝来自已被替换几何目标的 transitionend 事件。
  const transitionStartedAtRef = React.useRef(0);
  // transitionTargetCommittedRef keeps the initial ResizeObserver callback from skipping the start frame.
  // transitionTargetCommittedRef 防止初始 ResizeObserver 回调跳过动画起始帧。
  const transitionTargetCommittedRef = React.useRef(false);
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

  // resolveSurfaceId binds layout completion to the exact main or child transcript surface.
  // resolveSurfaceId 将布局完成事件绑定到精确的主会话或子会话转录界面。
  const resolveSurfaceId = React.useCallback((): string | null => {
    // owner falls back to the focused fold control while an opening body has not mounted yet.
    // owner 在展开正文尚未挂载时回退到当前聚焦的折叠控件。
    const owner = innerRef.current
      ?? (document.activeElement instanceof Element ? document.activeElement : null);
    return owner
      ?.closest<HTMLElement>("[data-conversation-surface-id]")
      ?.dataset.conversationSurfaceId ?? null;
  }, []);

  const finishTransition = React.useCallback(() => {
    fallbackTimerRef.current = null;
    collapsingRef.current = false;
    // shouldNotify closes only a transaction that actually entered animated geometry mutation.
    // shouldNotify 仅结束真实进入动画几何修改的事务。
    const shouldNotify = transitioningRef.current;
    transitioningRef.current = false;
    transitionTargetCommittedRef.current = false;
    if (expandedRef.current) {
      // Stable expanded shells return to native flow so a nested fold cannot animate every open ancestor again.
      // 稳定展开后的外壳回到原生流，避免嵌套折叠再次驱动所有已展开祖先执行动画。
      setTargetHeight(null);
      setVisualOpen(true);
      setHeight("auto");
      if (shouldNotify) {
        dispatchConversationLayoutSettled(resolveSurfaceId());
      }
      return;
    }
    setTargetHeight(collapsedHeight);
    setVisualOpen(false);
    setHeight(`${collapsedHeight}px`);
    setRendered(collapsedHeight > 0);
    if (shouldNotify) {
      dispatchConversationLayoutSettled(resolveSurfaceId());
    }
  }, [collapsedHeight, resolveSurfaceId]);

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
      transitioningRef.current = false;
      transitionTargetCommittedRef.current = false;
      setRendered(props.expanded || collapsedHeight > 0);
      setVisualOpen(props.expanded);
      setTargetHeight(props.expanded ? null : collapsedHeight);
      setHeight(props.expanded ? "auto" : `${collapsedHeight}px`);
      dispatchConversationLayoutSettled(resolveSurfaceId());
      return;
    }

    if (props.expanded) {
      collapsingRef.current = false;
      transitioningRef.current = true;
      transitionTargetCommittedRef.current = false;
      transitionStartedAtRef.current = Date.now();
      setRendered(true);
      setVisualOpen(false);
      setTargetHeight(collapsedHeight);
      setHeight(`${collapsedHeight}px`);
      frameRef.current = window.requestAnimationFrame(() => {
        frameRef.current = null;
        // startHeightRead forces the collapsed style into layout before the expanded target is committed.
        // startHeightRead 在提交展开目标前强制折叠样式进入布局计算。
        const startHeightRead = innerRef.current?.parentElement?.getBoundingClientRect().height;
        void startHeightRead;
        transitionTargetCommittedRef.current = true;
        const nextHeight = measureExpandedHeight();
        setTargetHeight(nextHeight);
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
    transitioningRef.current = true;
    transitionTargetCommittedRef.current = true;
    transitionStartedAtRef.current = Date.now();
    setTargetHeight(collapsedHeight);
    setVisualOpen(false);
    setHeight(`${currentHeight}px`);
    frameRef.current = window.requestAnimationFrame(() => {
      frameRef.current = null;
      // startHeightRead forces the current expanded height into layout before the collapsed target is committed.
      // startHeightRead 在提交折叠目标前强制当前展开高度进入布局计算。
      const startHeightRead = innerRef.current?.parentElement?.getBoundingClientRect().height;
      void startHeightRead;
      setVisualOpen(false);
      setHeight(`${collapsedHeight}px`);
      scheduleTransitionFinish();
    });
  }, [clearScheduledWork, collapsedHeight, measureExpandedHeight, prefersReducedMotion, props.expanded, rendered, resolveSurfaceId, scheduleTransitionFinish]);

  React.useLayoutEffect(() => {
    // The primary transition effect owns the closing phase; this effect only synchronizes a stable collapsed height.
    // 主过渡 effect 负责收起阶段；此 effect 仅同步稳定折叠高度。
    if (props.expanded || collapsingRef.current) {
      return;
    }
    setTargetHeight(collapsedHeight);
    setHeight(`${collapsedHeight}px`);
    setRendered(collapsedHeight > 0);
  }, [collapsedHeight, props.expanded]);

  React.useLayoutEffect(() => {
    if (!rendered || typeof ResizeObserver === "undefined") {
      return;
    }
    const element = innerRef.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(() => {
      if (!expandedRef.current || !transitioningRef.current || !transitionTargetCommittedRef.current) {
        return;
      }
      const nextHeight = element.scrollHeight;
      if (heightRef.current !== `${nextHeight}px`) {
        // An in-flight body may refine its one active target, but it must not restart the whole settling window.
        // 动画中的正文可以修正当前唯一目标，但不得重新开始整段稳定等待窗口。
        setTargetHeight(nextHeight);
        setHeight(`${nextHeight}px`);
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [rendered]);

  React.useEffect(() => () => {
    clearScheduledWork();
    if (transitioningRef.current) {
      transitioningRef.current = false;
      dispatchConversationLayoutSettled(resolveSurfaceId());
    }
  }, [clearScheduledWork, resolveSurfaceId]);

  if (!rendered) {
    return null;
  }

  return (
    <div
      aria-hidden={!props.expanded && collapsedHeight <= 0}
      className={cn("vk-collapse", props.className)}
      data-open={visualOpen ? "" : undefined}
      data-conversation-measured-collapse-id={props.collapseId}
      data-conversation-measured-target-height={targetHeight === null ? undefined : String(targetHeight)}
      onTransitionEnd={(event) => {
        if (event.currentTarget !== event.target || event.propertyName !== "height") {
          return;
        }
        if (Date.now() - transitionStartedAtRef.current < CONVERSATION_MEASURED_COLLAPSE_MS - 20) {
          return;
        }
        // expectedHeight verifies this transitionend belongs to the latest retargeted geometry.
        // expectedHeight 验证当前 transitionend 属于最近一次重定向后的几何目标。
        const expectedHeight = expandedRef.current
          ? measureExpandedHeight()
          : collapsedHeight;
        if (Math.abs(event.currentTarget.getBoundingClientRect().height - expectedHeight) > 1) {
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
      <div className="vk-collapse-inner" ref={innerRef}>
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
  return leftKeys.every((key) => {
    // leftMemory and rightMemory must both exist because missing keys represent different authoritative state.
    // leftMemory 与 rightMemory 必须同时存在，因为缺失键表示不同的权威状态。
    const leftMemory = left[key];
    const rightMemory = right[key];
    return Boolean(leftMemory && rightMemory && conversationMeasuredCollapseMemoryEqual(leftMemory, rightMemory));
  });
}

// conversationMeasuredCollapseMemoryEqual compares the persisted fields for one collapse node.
// conversationMeasuredCollapseMemoryEqual 比较单个折叠节点的持久化字段。
function conversationMeasuredCollapseMemoryEqual(
  left: ConversationMeasuredCollapseMemory,
  right: ConversationMeasuredCollapseMemory,
): boolean {
  return left.expanded === right.expanded;
}
