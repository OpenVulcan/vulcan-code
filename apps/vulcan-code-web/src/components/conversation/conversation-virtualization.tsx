import React from "react";

// ConversationTimelineVirtualWindow describes the currently mounted timeline slice.
// ConversationTimelineVirtualWindow 描述当前挂载的时间线切片。
export interface ConversationTimelineVirtualWindow<TItem> {
  // Mounted items with their original list indexes.
  // 携带原始列表索引的已挂载条目。
  items: Array<{ index: number; item: TItem }>;
  // Spacer height before the mounted slice.
  // 已挂载切片之前的占位高度。
  paddingTop: number;
  // Spacer height after the mounted slice.
  // 已挂载切片之后的占位高度。
  paddingBottom: number;
  // Whether the window is currently using measured row heights.
  // 当前窗口是否正在使用测量行高。
  measuring: boolean;
  // Whether the full list is currently virtualized.
  // 当前完整列表是否已启用虚拟化。
  virtualized: boolean;
}

// ConversationTimelineVirtualRestoreState stores reusable layout measurements for one transcript surface.
// ConversationTimelineVirtualRestoreState 保存单个转录界面可复用的布局测量信息。
export interface ConversationTimelineVirtualRestoreState {
  // Previously measured row heights keyed by stable timeline row keys.
  // 按稳定时间线行键保存的历史测量行高。
  rowHeightsByKey: Record<string, number>;
}

// CONVERSATION_VIRTUALIZATION_MIN_ITEMS avoids adding measurement wrappers to short transcripts.
// CONVERSATION_VIRTUALIZATION_MIN_ITEMS 避免给短转录添加测量包装层。
const CONVERSATION_VIRTUALIZATION_MIN_ITEMS = 80;

// CONVERSATION_VIRTUALIZATION_OVERSCAN_PX keeps nearby transcript rows mounted around the viewport.
// CONVERSATION_VIRTUALIZATION_OVERSCAN_PX 让视口附近的转录行保持挂载。
const CONVERSATION_VIRTUALIZATION_OVERSCAN_PX = 1200;

// conversationDistanceFromBottom returns the scroll metric used by virtual transcript restoration.
// conversationDistanceFromBottom 返回虚拟转录恢复使用的底部距离指标。
function conversationDistanceFromBottom(element: HTMLElement): number {
  return Math.max(0, element.scrollHeight - element.scrollTop - element.clientHeight);
}

// conversationScrollToDistanceFromBottom restores a visual bottom distance in the current layout.
// conversationScrollToDistanceFromBottom 在当前布局中恢复视觉底部距离。
function conversationScrollToDistanceFromBottom(element: HTMLElement, distanceFromBottom: number): void {
  element.scrollTop = Math.max(0, element.scrollHeight - element.clientHeight - distanceFromBottom);
}

// useConversationTimelineVirtualWindow returns the mounted range for a long transcript.
// useConversationTimelineVirtualWindow 返回长转录当前需要挂载的范围。
export function useConversationTimelineVirtualWindow<TItem>(
  // Full timeline items projected by the caller.
  // 调用方投影出的完整时间线条目。
  items: TItem[],
  // Scroll container that owns the transcript viewport.
  // 承载转录视口的滚动容器。
  scrollRef: React.RefObject<HTMLElement | null>,
  // Stable cache scope key for one transcript surface.
  // 单个转录界面的稳定缓存范围键。
  cacheScopeKey: string,
  // Initial distance from the viewport bottom before measurement catches up.
  // 测量追上前使用的初始视口底部距离。
  initialDistanceFromBottom: number,
  // Previously stored row-height measurements for this surface.
  // 当前界面此前保存的行高测量数据。
  initialVirtualizerState: ConversationTimelineVirtualRestoreState | null,
  // Persists updated row-height measurements.
  // 持久化更新后的行高测量数据。
  onVirtualizerStateChange: (surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => void,
  // Returns a stable measurement key for one item.
  // 返回单个条目的稳定测量键。
  itemKey: (item: TItem) => string,
  // Returns a conservative initial height by item kind.
  // 按条目类型返回保守初始高度。
  estimateItemHeight: (item: TItem) => number,
): {
  // Current virtual window projection.
  // 当前虚拟窗口投影。
  window: ConversationTimelineVirtualWindow<TItem>;
  // Refreshes viewport metrics after a scroll event.
  // 滚动事件后刷新视口指标。
  onScroll: () => void;
  // Stores a measured row height by stable timeline key.
  // 按稳定时间线键保存测量后的行高。
  onRowHeight: (key: string, height: number) => void;
} {
  const measuredHeightsRef = React.useRef<Map<string, number>>(
    new Map(Object.entries(initialVirtualizerState?.rowHeightsByKey ?? {})),
  );
  const cacheScopeKeyRef = React.useRef(cacheScopeKey);
  const pendingInitialDistanceFromBottomRef = React.useRef<number | null>(Math.max(0, initialDistanceFromBottom));
  const [measureVersion, setMeasureVersion] = React.useState(0);
  const [viewport, setViewport] = React.useState({ scrollTop: 0, height: 0, width: 0 });
  if (cacheScopeKeyRef.current !== cacheScopeKey) {
    measuredHeightsRef.current = new Map(Object.entries(initialVirtualizerState?.rowHeightsByKey ?? {}));
    cacheScopeKeyRef.current = cacheScopeKey;
    pendingInitialDistanceFromBottomRef.current = Math.max(0, initialDistanceFromBottom);
  }
  const onScroll = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    setViewport({
      scrollTop: element.scrollTop,
      height: element.clientHeight,
      width: element.clientWidth,
    });
  }, [scrollRef]);

  React.useLayoutEffect(() => {
    onScroll();
  }, [items.length, onScroll]);

  React.useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", onScroll);
      return () => window.removeEventListener("resize", onScroll);
    }
    const observer = new ResizeObserver(onScroll);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onScroll, scrollRef]);

  React.useLayoutEffect(() => {
    if (pendingInitialDistanceFromBottomRef.current === null) {
      return;
    }
    let cancelled = false;
    let frame: number | null = null;
    // restorePendingBottomDistance aligns the DOM scroll position with the virtual projection.
    // restorePendingBottomDistance 让 DOM 滚动位置与虚拟窗口投影保持一致。
    const restorePendingBottomDistance = () => {
      const element = scrollRef.current;
      const distanceFromBottom = pendingInitialDistanceFromBottomRef.current;
      if (!element || distanceFromBottom === null) {
        return;
      }
      if (distanceFromBottom <= 0) {
        element.scrollTop = element.scrollHeight;
        return;
      }
      conversationScrollToDistanceFromBottom(element, distanceFromBottom);
    };
    const clearAfterFrame = (remainingFrames: number) => {
      frame = window.requestAnimationFrame(() => {
        if (cancelled) {
          return;
        }
        restorePendingBottomDistance();
        if (remainingFrames <= 0) {
          pendingInitialDistanceFromBottomRef.current = null;
          onScroll();
          return;
        }
        clearAfterFrame(remainingFrames - 1);
      });
    };
    clearAfterFrame(3);
    return () => {
      cancelled = true;
      if (frame !== null) {
        window.cancelAnimationFrame(frame);
      }
    };
  }, [cacheScopeKey, onScroll, scrollRef]);

  React.useEffect(() => {
    let changed = false;
    const liveKeys = new Set(items.map(itemKey));
    for (const key of measuredHeightsRef.current.keys()) {
      if (!liveKeys.has(key)) {
        measuredHeightsRef.current.delete(key);
        changed = true;
      }
    }
    if (changed) {
      setMeasureVersion((version) => version + 1);
      onVirtualizerStateChange(cacheScopeKey, {
        rowHeightsByKey: Object.fromEntries(measuredHeightsRef.current),
      });
    }
  }, [cacheScopeKey, itemKey, items, onVirtualizerStateChange]);

  const onRowHeight = React.useCallback((key: string, height: number) => {
    if (!Number.isFinite(height) || height <= 0) {
      return;
    }
    const rounded = Math.ceil(height);
    const current = measuredHeightsRef.current.get(key);
    if (current === rounded) {
      return;
    }
    const scrollElement = scrollRef.current;
    const heightDelta = current == null ? 0 : rounded - current;
    const measuredHeights = measuredHeightsRef.current;
    const rowIndex = items.findIndex((item) => itemKey(item) === key);
    const rowItem = rowIndex < 0 ? null : items[rowIndex] ?? null;
    const rowTopBefore = rowIndex < 0
      ? null
      : items.slice(0, rowIndex).reduce((sum, item) => (
        sum + (measuredHeights.get(itemKey(item)) ?? estimateItemHeight(item))
      ), 0);
    const rowBottomBefore = rowTopBefore == null
      ? null
      : rowTopBefore + (current ?? (rowItem ? estimateItemHeight(rowItem) : 0));
    const distanceFromBottom = scrollElement
      ? conversationDistanceFromBottom(scrollElement)
      : Number.POSITIVE_INFINITY;
    const wasNearBottom = scrollElement
      ? distanceFromBottom <= 24 || (heightDelta > 0 && distanceFromBottom - heightDelta <= 24)
      : false;
    const shouldPreserveViewport = Boolean(
      scrollElement
      && heightDelta !== 0
      && rowBottomBefore != null
      && rowBottomBefore <= scrollElement.scrollTop + 1,
    );
    measuredHeightsRef.current.set(key, rounded);
    onVirtualizerStateChange(cacheScopeKey, {
      rowHeightsByKey: Object.fromEntries(measuredHeightsRef.current),
    });
    setMeasureVersion((version) => version + 1);
    if (!scrollElement) {
      return;
    }
    if (wasNearBottom) {
      window.requestAnimationFrame(() => {
        scrollElement.scrollTop = scrollElement.scrollHeight;
      });
      return;
    }
    if (shouldPreserveViewport) {
      scrollElement.scrollTop += heightDelta;
    }
  }, [cacheScopeKey, estimateItemHeight, itemKey, items, onVirtualizerStateChange, scrollRef]);

  const virtualWindow = React.useMemo<ConversationTimelineVirtualWindow<TItem>>(() => {
    void measureVersion;
    if (items.length < CONVERSATION_VIRTUALIZATION_MIN_ITEMS || viewport.height <= 0) {
      return {
        items: items.map((item, index) => ({ index, item })),
        paddingTop: 0,
        paddingBottom: 0,
        measuring: false,
        virtualized: false,
      };
    }
    const heights = items.map((item) => measuredHeightsRef.current.get(itemKey(item))
      ?? estimateItemHeight(item));
    const offsets: number[] = [];
    let totalHeight = 0;
    for (const height of heights) {
      offsets.push(totalHeight);
      totalHeight += height;
    }
    const pendingDistanceFromBottom = pendingInitialDistanceFromBottomRef.current;
    const effectiveScrollTop = pendingDistanceFromBottom === null
      ? viewport.scrollTop
      : Math.max(0, totalHeight - viewport.height - pendingDistanceFromBottom);
    const windowTop = Math.max(0, effectiveScrollTop - CONVERSATION_VIRTUALIZATION_OVERSCAN_PX);
    const windowBottom = effectiveScrollTop + viewport.height + CONVERSATION_VIRTUALIZATION_OVERSCAN_PX;
    let startIndex = 0;
    while (startIndex < items.length && offsets[startIndex] + heights[startIndex] < windowTop) {
      startIndex += 1;
    }
    let endIndex = startIndex;
    while (endIndex < items.length && offsets[endIndex] <= windowBottom) {
      endIndex += 1;
    }
    const safeStart = Math.max(0, Math.min(startIndex, items.length - 1));
    const safeEnd = Math.max(safeStart + 1, Math.min(items.length, endIndex));
    const paddingTop = offsets[safeStart] ?? 0;
    const mountedHeight = heights.slice(safeStart, safeEnd).reduce((sum, height) => sum + height, 0);
    const paddingBottom = Math.max(0, totalHeight - paddingTop - mountedHeight);
    return {
      items: items.slice(safeStart, safeEnd).map((item, offset) => ({ index: safeStart + offset, item })),
      paddingTop,
      paddingBottom,
      measuring: true,
      virtualized: true,
    };
  }, [cacheScopeKey, estimateItemHeight, itemKey, items, measureVersion, viewport.height, viewport.scrollTop, viewport.width]);
  return { window: virtualWindow, onScroll, onRowHeight };
}

// ConversationTimelineVirtualRow measures one mounted timeline entry for the virtual window.
// ConversationTimelineVirtualRow 为虚拟窗口测量单条已挂载时间线记录。
export function ConversationTimelineVirtualRow(props: {
  // Stable row key used for height caching.
  // 用于高度缓存的稳定行键。
  rowKey: string;
  // Notifies the virtualizer when the row height changes.
  // 行高变化时通知虚拟列表。
  onHeight: (key: string, height: number) => void;
  // Mounted timeline content.
  // 已挂载的时间线内容。
  children: React.ReactNode;
}) {
  const elementRef = React.useRef<HTMLDivElement | null>(null);
  const { onHeight, rowKey } = props;
  React.useLayoutEffect(() => {
    const element = elementRef.current;
    if (!element) {
      return;
    }
    const updateHeight = () => {
      onHeight(rowKey, element.getBoundingClientRect().height);
    };
    updateHeight();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(updateHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onHeight, rowKey]);
  return (
    <div className="timeline-virtual-row" ref={elementRef}>
      {props.children}
    </div>
  );
}
