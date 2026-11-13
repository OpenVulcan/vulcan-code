import React from "react";
import { flushSync } from "react-dom";
import { captureConversationViewportAnchor } from "./chat-area-scroll";
import { CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, type ConversationTurnWindowLayoutDetail } from "./conversation-layout-events";

// Keep adjacent turns ready before they cross the viewport edge.
// 在相邻回合跨入视口前提前挂载。
const TURN_OVERSCAN_PX = 600;

// Give unseen turns enough scroll space until their bodies are first measured.
// 为尚未测量正文的回合预留初始滚动空间。
export const ESTIMATED_TURN_HEIGHT_PX = 480;

// The find bar and transcript Select All request the complete loaded DOM for different lifetimes.
// 查找栏和转录全选在各自的有效期内请求完整的已加载 DOM。
export const CONVERSATION_TURN_WINDOW_EXPAND_EVENT = "vulcan:conversation-turn-window-expand";

// ConversationTurnWindowRequest identifies which exact DOM operation needs all loaded text.
// ConversationTurnWindowRequest 标识哪种 DOM 操作需要全部已加载文本。
export interface ConversationTurnWindowRequest {
  // Find stays active until its bar closes; selection stays active until the range clears.
  // 查找持续到查找栏关闭；全选持续到选区清除。
  reason: "find" | "selection";
  // Whether the requesting operation still needs all turns.
  // 请求方是否仍需要全部回合。
  active: boolean;
}

// useConversationTurnWindow observes persistent turn shells and returns the body keys and measured placeholder heights.
// useConversationTurnWindow 观察持久回合外壳，返回应挂载的正文键及占位实测高度。
// Parameters: scrollRef owns the viewport and rowKeys is the current order of the shells.
// 参数：scrollRef 持有视口，rowKeys 为当前回合外壳顺序。
export function useConversationTurnWindow(
  scrollRef: React.RefObject<HTMLElement | null> | undefined,
  rowKeys: readonly string[],
): {
  // Null indicates that all turns should mount for a DOM-based action or an unsupported browser.
  // null 表示为 DOM 操作或缺少浏览器能力的环境挂载全部回合。
  mounted: ReadonlySet<string> | null;
  // Last measured height for a previously mounted body.
  // 曾挂载正文的上一次实测高度。
  heights: ReadonlyMap<string, number>;
} {
  // Observer support is also checked in the geometry-free rendering environment used by existing tests.
  // 既有测试使用无几何布局的渲染环境，也通过浏览器能力检查处理。
  const enabled = Boolean(scrollRef && typeof IntersectionObserver !== "undefined");
  // A mounted set keeps only in-range bodies and any body owning focus or selection.
  // 挂载集合只保留缓冲区内正文及拥有焦点或选区的正文。
  const [mounted, setMounted] = React.useState<ReadonlySet<string> | null>(() => enabled ? new Set() : null);
  // Keep observer state outside React so reading geometry does not schedule a render.
  // 将观察器状态置于 React 外，避免读取几何时触发渲染。
  const mountedRef = React.useRef(mounted);
  const heights = React.useRef(new Map<string, number>());
  // Sync shell registrations after an append or prepend without resetting the observer's viewport state.
  // 追加或前插后同步外壳注册，保留观察器原有视口状态。
  const syncShells = React.useRef<(() => void) | null>(null);

  React.useEffect(() => {
    // The exact viewport and timeline are bound together for the entire observer lifetime.
    // 在观察器生命周期内绑定同一个精确视口和时间线。
    const scrollport = scrollRef?.current;
    const timeline = scrollport?.querySelector<HTMLElement>(".vk-transcript-inner > .vk-timeline");
    if (!enabled || !scrollport || !timeline) return;

    // Shells survive both mounting and eviction of their message bodies.
    // 无论正文挂载还是卸载，回合外壳都保留。
    const shells = new Map<Element, { element: HTMLElement; key: string }>();
    // Intersection records are keyed by shell identity so removed turns cannot reappear.
    // 按外壳身份记录相交状态，防止已移除的回合再次挂载。
    const nearby = new Set<string>();
    // Independent DOM operations can overlap without prematurely evicting text.
    // 多种 DOM 操作可以重叠，避免提前卸载正文。
    const requests = new Set<ConversationTurnWindowRequest["reason"]>();

    // updateWindow computes the desired set, measures evictions, and commits once for the whole observer batch.
    // updateWindow 计算目标集合，测量即将卸载的回合，并为整个观察器批次一次性提交。
    const updateWindow = (): void => {
      // Capture focus and selection only for mounted shells to preserve ongoing interactions.
      // 保留已挂载且持有焦点或选区的外壳，避免中断正在进行的操作。
      const active = timeline.ownerDocument.activeElement;
      const selection = timeline.ownerDocument.getSelection();
      const range = selection && !selection.isCollapsed && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
      // previous is the committed body set; null represents an explicit full DOM.
      // previous 是已提交正文集合；null 表示显式完整 DOM。
      const previous = mountedRef.current;
      const next = requests.size > 0 ? null : new Set<string>();
      if (next) {
        for (const key of nearby) next.add(key);
        if (timeline.contains(active) || range?.intersectsNode(timeline)) {
          for (const { element: shell, key } of shells.values()) {
            if ((previous === null || previous.has(key)) && (shell.contains(active) || range?.intersectsNode(shell))) {
              next.add(key);
            }
          }
        }
      }
      if (previous === next || (previous && next && previous.size === next.size && [...previous].every((key) => next.has(key)))) {
        return;
      }

      // Save live heights just before eviction, including any streamed or asynchronously rendered content.
      // 在卸载前记录实时高度，涵盖流式输出或异步渲染的内容。
      for (const { element: shell, key } of shells.values()) {
        if (next && !next.has(key) && (previous === null || previous.has(key))) {
          const height = shell.getBoundingClientRect().height;
          heights.current.set(key, height);
        }
      }

      // Capture the stable shell before replacing estimated heights, then let the scroll owner correct it.
      // 替换估算高度前捕获稳定外壳，随后由滚动所有者校正位置。
      const anchor = captureConversationViewportAnchor(scrollport);
      mountedRef.current = next;
      flushSync(() => setMounted(next));
      scrollport.dispatchEvent(new CustomEvent<ConversationTurnWindowLayoutDetail>(
        CONVERSATION_TURN_WINDOW_LAYOUT_EVENT,
        { detail: { anchor } },
      ));
    };

    // Use one native observer for every shell, with a buffer in CSS pixels on either side.
    // 全部外壳共享一个原生观察器，视口上下以 CSS 像素设置缓冲。
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        // The registered shell is the source of the key, even after its body unmounts.
        // 即使正文卸载，已注册的外壳仍是回合键的来源。
        const shell = shells.get(entry.target);
        if (!shell) continue;
        if (entry.isIntersecting) nearby.add(shell.key);
        else nearby.delete(shell.key);
      }
      updateWindow();
    }, { root: scrollport, rootMargin: `${TURN_OVERSCAN_PX}px 0px` });

    // Register new shells and release cached measurements for rows removed from the window.
    // 注册新增外壳，并释放已移出数据窗口的回合高度缓存。
    const sync = (): void => {
      for (const { element: shell, key } of shells.values()) {
        if (shell.parentElement === timeline) continue;
        observer.unobserve(shell);
        shells.delete(shell);
        nearby.delete(key);
        heights.current.delete(key);
      }
      for (const child of timeline.children) {
        if (!(child instanceof HTMLElement) || shells.has(child)) continue;
        // Only exact turn shells belong to the observer; other timeline controls stay mounted.
        // 仅精确回合外壳归观察器管理，其余时间线控件保持挂载。
        const key = child.dataset.conversationTurnShell;
        if (key === undefined) continue;
        shells.set(child, { element: child, key });
        observer.observe(child);
      }
    };
    // The request listener is scoped to this scrollport and never expands another conversation.
    // 请求监听器仅作用于此滚动视口，不会展开其他会话。
    const onRequest = (event: Event): void => {
      const { active, reason } = (event as CustomEvent<ConversationTurnWindowRequest>).detail;
      if (active) requests.add(reason);
      else requests.delete(reason);
      updateWindow();
    };
    // Recheck eviction after the user finishes selecting or leaves a focused control.
    // 用户结束选区或离开聚焦控件后重新检查卸载范围。
    const onInteractionChange = (): void => {
      // A selection moved outside this transcript also releases the full DOM.
      // 选区移出当前转录时同样释放完整 DOM。
      const selection = timeline.ownerDocument.getSelection();
      if (!selection || selection.isCollapsed || !selection.getRangeAt(0).intersectsNode(timeline)) {
        requests.delete("selection");
      }
      updateWindow();
    };
    // Coalesce focus transitions until the new active element is visible to the observer logic.
    // 将焦点变化合并到新活动元素可供观察逻辑读取之后。
    let interactionFrame = 0;
    const scheduleInteractionChange = (): void => {
      if (interactionFrame) return;
      interactionFrame = window.requestAnimationFrame(() => {
        interactionFrame = 0;
        onInteractionChange();
      });
    };
    syncShells.current = sync;
    sync();
    scrollport.addEventListener(CONVERSATION_TURN_WINDOW_EXPAND_EVENT, onRequest);
    timeline.addEventListener("focusout", scheduleInteractionChange);
    timeline.ownerDocument.addEventListener("selectionchange", scheduleInteractionChange);
    return () => {
      syncShells.current = null;
      observer.disconnect();
      scrollport.removeEventListener(CONVERSATION_TURN_WINDOW_EXPAND_EVENT, onRequest);
      timeline.removeEventListener("focusout", scheduleInteractionChange);
      timeline.ownerDocument.removeEventListener("selectionchange", scheduleInteractionChange);
      window.cancelAnimationFrame(interactionFrame);
    };
  }, [enabled, scrollRef]);

  React.useEffect(() => {
    syncShells.current?.();
  }, [rowKeys]);

  return { mounted, heights: heights.current };
}
