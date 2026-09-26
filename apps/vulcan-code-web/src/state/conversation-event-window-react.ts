import React from "react";

import type {
  ConversationEventWindowController,
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "@/service/conversation-event-window-controller";
import type {
  SelectedConversationEventWindowCommands,
  SelectedConversationEventWindowStatus,
} from "@/service/selected-conversation-event-window-registry";

/** Minimal controller surface required by React's external-store protocol.
 * React 外部 store 协议所需的最小控制器表面。 */
export type ConversationEventWindowReadableStore = Pick<
  ConversationEventWindowController,
  "snapshot" | "subscribe"
>;

/** Selected-session owner surface that exposes snapshot reads and bound commands.
 * 同时暴露快照读取与绑定命令的选中会话归属表面。 */
export interface ConversationEventWindowOwnedStore
  extends ConversationEventWindowReadableStore {
  /** Returns commands bound to the current selected-session controller generation.
   * 返回绑定到当前选中会话控制器代次的命令。 */
  commands(): SelectedConversationEventWindowCommands;
  /** Returns the current selected-session hydration lifecycle.
   * 返回当前选中会话 hydration 生命周期。 */
  status(): SelectedConversationEventWindowStatus;
}

/** Stable adapter consumed by React useSyncExternalStore.
 * React useSyncExternalStore 消费的稳定适配器。 */
export interface ConversationEventWindowExternalStore {
  /** Subscribes to complete atomic controller publications.
   * 订阅控制器完整原子发布。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void;
  /** Returns the latest stable client snapshot.
   * 返回最近的稳定客户端快照。 */
  getSnapshot(): ConversationEventWindowControllerSnapshot;
  /** Returns the same authoritative snapshot during server rendering.
   * 在服务端渲染期间返回同一份权威快照。 */
  getServerSnapshot(): ConversationEventWindowControllerSnapshot;
  /** Returns commands bound to the selected owner visible in the current snapshot.
   * 返回绑定到当前快照可见选中归属的命令。 */
  getCommands(): SelectedConversationEventWindowCommands;
  /** Returns the stable selected-session hydration lifecycle.
   * 返回稳定的选中会话 hydration 生命周期。 */
  getStatus(): SelectedConversationEventWindowStatus;
}

/** React owner boundary for the selected-session V1 external store.
 * 选中会话 V1 外部 store 的 React 归属边界。 */
const ConversationEventWindowExternalStoreContext =
  React.createContext<ConversationEventWindowExternalStore | null>(null);

/** Longest wait before a coalesced publication flushes when no animation frame arrives, as in hidden
 * or throttled windows.
 * 未收到动画帧（窗口隐藏或被节流）时，合并发布在刷新前最多等待的时长。 */
export const CONVERSATION_EVENT_WINDOW_FRAME_FALLBACK_MS = 50;

/** Schedules one flush and returns a callback that cancels it before it runs.
 * 调度一次刷新，并返回可在其执行前取消它的回调。 */
export type ConversationEventWindowFrameScheduler = (flush: () => void) => () => void;

/** Schedules one flush on the next animation frame, or after the fallback delay when frames stall.
 * 在下一动画帧调度一次刷新；动画帧停滞时在兜底延迟后刷新。
 *
 * @param flush callback run exactly once unless cancelled first.
 *              除非先被取消，否则恰好执行一次的回调。
 * @returns idempotent cancellation callback.
 *          幂等的取消回调。 */
export function scheduleConversationEventWindowFrame(flush: () => void): () => void {
  let settled = false;
  const frame = typeof requestAnimationFrame === "function" ? requestAnimationFrame(() => run()) : null;
  const timer = setTimeout(() => run(), CONVERSATION_EVENT_WINDOW_FRAME_FALLBACK_MS);
  const cancel = () => {
    settled = true;
    if (frame !== null) cancelAnimationFrame(frame);
    clearTimeout(timer);
  };
  function run() {
    if (settled) return;
    cancel();
    flush();
  }
  return cancel;
}

/** Frame-coalesced view over one notifying source.
 * 基于单个通知来源的按帧合并视图。 */
interface FrameCoalescedPublication<T> {
  /** Subscribes to at most one notification per scheduled frame.
   * 订阅每个调度帧至多一次的通知。 */
  subscribe(listener: ConversationEventWindowControllerListener): () => void;
  /** Returns the value visible since the latest flush, or the live value while nobody subscribes.
   * 返回最近一次刷新后的可见值；无人订阅时返回实时值。 */
  read(): T;
}

/** Coalesces bursts of source notifications into one publication per frame.
 * 将来源的突发通知合并为每帧一次发布。
 *
 * Streaming deltas can arrive faster than one full transcript render, and useSyncExternalStore renders
 * synchronously on every notification, so an uncoalesced burst queues renders until the UI falls minutes
 * behind. Publishing only the latest value per frame bounds the render rate by the display, not the stream.
 * 流式增量的到达速度可能快于一次完整转录渲染，而 useSyncExternalStore 会在每次通知时同步渲染，未合并的
 * 突发通知会堆积渲染，使界面落后数分钟。每帧只发布最新值，使渲染频率受显示刷新约束而不是受流速约束。
 *
 * @param subscribeSource subscribes to raw source notifications.
 *                        订阅原始来源通知。
 * @param readSource reads the current complete source value.
 *                   读取当前完整来源值。
 * @param scheduleFrame schedules the coalesced flush.
 *                      调度合并后的刷新。
 * @returns the coalesced publication.
 *          合并后的发布视图。 */
function createFrameCoalescedPublication<T>(
  subscribeSource: (listener: ConversationEventWindowControllerListener) => () => void,
  readSource: () => T,
  scheduleFrame: ConversationEventWindowFrameScheduler,
): FrameCoalescedPublication<T> {
  const listeners = new Set<ConversationEventWindowControllerListener>();
  // visible holds the value published by the latest flush while at least one listener is subscribed.
  // visible 在至少存在一个监听器期间保存最近一次刷新发布的值。
  let visible: { value: T } | null = null;
  let releaseSource: (() => void) | null = null;
  let cancelFrame: (() => void) | null = null;
  const flush = () => {
    cancelFrame = null;
    visible = { value: readSource() };
    for (const listener of Array.from(listeners)) listener();
  };
  return {
    subscribe(listener) {
      listeners.add(listener);
      if (releaseSource === null) {
        visible = { value: readSource() };
        releaseSource = subscribeSource(() => {
          if (cancelFrame === null) cancelFrame = scheduleFrame(flush);
        });
      }
      return () => {
        if (!listeners.delete(listener) || listeners.size > 0) return;
        releaseSource?.();
        releaseSource = null;
        cancelFrame?.();
        cancelFrame = null;
        visible = null;
      };
    },
    read: () => visible === null ? readSource() : visible.value,
  };
}

/** One selected-owner publication whose snapshot, lifecycle and commands always change together.
 * 快照、生命周期与命令始终同步变化的单次选中归属发布。 */
interface SelectedConversationEventWindowPublication {
  snapshot: ConversationEventWindowControllerSnapshot;
  status: SelectedConversationEventWindowStatus;
  commands: SelectedConversationEventWindowCommands;
}

/** Creates one React external-store adapter around a V1 window controller.
 * 围绕 V1 窗口控制器创建一个 React 外部 store 适配器。
 *
 * @param controller controller that owns the canonical event window.
 *                   持有 canonical 事件窗口的控制器。
 * @param scheduleFrame schedules coalesced publications; tests inject a manual scheduler.
 *                      调度合并发布；测试可注入手动调度器。
 * @returns stable external-store callbacks bound to that controller.
 *          绑定到该控制器的稳定外部 store 回调。 */
export function createConversationEventWindowExternalStore(
  controller: ConversationEventWindowOwnedStore,
  scheduleFrame: ConversationEventWindowFrameScheduler = scheduleConversationEventWindowFrame,
): ConversationEventWindowExternalStore {
  const publication = createFrameCoalescedPublication<SelectedConversationEventWindowPublication>(
    (listener) => controller.subscribe(listener),
    () => ({ snapshot: controller.snapshot(), status: controller.status(), commands: controller.commands() }),
    scheduleFrame,
  );
  return {
    subscribe: publication.subscribe,
    getSnapshot: () => publication.read().snapshot,
    getServerSnapshot: () => publication.read().snapshot,
    getCommands: () => publication.read().commands,
    getStatus: () => publication.read().status,
  };
}

/** Reads one controller through React's tear-free external-store boundary.
 * 通过 React 无撕裂外部 store 边界读取一个控制器。
 *
 * @param controller controller selected by the future production owner registry.
 *                   由后续生产 owner registry 选中的控制器。
 * @returns the latest atomic V1 event-window snapshot.
 *          最近一份原子 V1 事件窗口快照。 */
export function useConversationEventWindowSnapshot(
  controller: ConversationEventWindowReadableStore,
): ConversationEventWindowControllerSnapshot {
  // readableStore changes only when controller ownership changes, preserving callback identities across renders;
  // it publishes at most one snapshot per frame so streaming bursts cannot queue renders.
  // readableStore 仅在控制器归属变化时改变，从而跨渲染保持回调身份稳定；它每帧至多发布一份快照，使流式突发
  // 无法堆积渲染。
  const readableStore = React.useMemo(() => {
    const publication = createFrameCoalescedPublication(
      (listener) => controller.subscribe(listener),
      () => controller.snapshot(),
      scheduleConversationEventWindowFrame,
    );
    return {
      subscribe: publication.subscribe,
      getSnapshot: publication.read,
      getServerSnapshot: publication.read,
    };
  }, [controller]);
  return React.useSyncExternalStore(
    readableStore.subscribe,
    readableStore.getSnapshot,
    readableStore.getServerSnapshot,
  );
}

/** Provides the stable selected-session external store without rendering it.
 * 提供稳定的选中会话外部 store，但不负责渲染它。
 *
 * @param props stable store and descendants that may later consume the V1 window.
 *              稳定 store 与后续可以消费 V1 窗口的后代节点。
 * @returns the context owner surrounding its children.
 *          包围子节点的上下文归属方。 */
export function ConversationEventWindowExternalStoreProvider(props: {
  /** Stable external store owned by WorkbenchApp.
   * WorkbenchApp 持有的稳定外部 store。 */
  store: ConversationEventWindowExternalStore;
  /** Existing workbench subtree; no renderer authority changes here.
   * 既有工作台子树；此处不改变 renderer 权威。 */
  children: React.ReactNode;
}) {
  return React.createElement(
    ConversationEventWindowExternalStoreContext.Provider,
    { value: props.store },
    props.children,
  );
}

/** Returns the WorkbenchApp-owned selected-session external store.
 * 返回 WorkbenchApp 持有的选中会话外部 store。
 *
 * @returns stable subscribe/snapshot callbacks for the future single renderer cutover.
 *          用于后续单 renderer 切换的稳定 subscribe/snapshot 回调。 */
export function useSelectedConversationEventWindowExternalStore():
ConversationEventWindowExternalStore {
  const store = React.useContext(ConversationEventWindowExternalStoreContext);
  if (store === null) {
    throw new Error("selected conversation event window external store is unavailable");
  }
  return store;
}

/** Reads the selected-session V1 snapshot through the WorkbenchApp-owned external store.
 * 通过 WorkbenchApp 持有的外部 store 读取选中会话 V1 快照。
 *
 * @returns the latest atomic selected-session snapshot.
 *          最近的原子选中会话快照。 */
export function useSelectedConversationEventWindowSnapshot():
ConversationEventWindowControllerSnapshot {
  const store = useSelectedConversationEventWindowExternalStore();
  return React.useSyncExternalStore(
    store.subscribe,
    store.getSnapshot,
    store.getServerSnapshot,
  );
}

/** Reads one value derived from the selected-session V1 snapshot, re-rendering only when that value changes.
 * 读取从选中会话 V1 快照派生的单个值，仅在该值变化时重新渲染。
 *
 * Components that need a few facts, such as the owner id or the latest todolist, must use this instead of the full
 * snapshot hook: every streaming delta publishes a new snapshot, and a full-snapshot subscriber high in the tree
 * re-renders its whole subtree once per frame while a reply streams.
 * 只需要少量事实（例如归属 id 或最新待办清单）的组件必须使用本 hook 而不是完整快照 hook：每个流式增量都会发布
 * 新快照，位于树高处的完整快照订阅者会在回复流式输出期间每帧重新渲染其整个子树。
 *
 * @param selector derives the value; keep its identity stable (module scope or useCallback) to reuse the memo.
 *                 派生该值；保持其身份稳定（模块作用域或 useCallback）以复用记忆结果。
 * @param isEqual decides whether a newly derived value may keep the previous one; defaults to Object.is.
 *                判断新派生值能否沿用上一个值；默认使用 Object.is。
 * @returns the selected value, identical across snapshots for which isEqual holds.
 *          选中的值；在 isEqual 成立的各快照之间保持同一身份。 */
export function useSelectedConversationEventWindowSelector<T>(
  selector: (snapshot: ConversationEventWindowControllerSnapshot) => T,
  isEqual: (left: T, right: T) => boolean = Object.is,
): T {
  const store = useSelectedConversationEventWindowExternalStore();
  // readSelection memoizes by snapshot identity and keeps the previous value while isEqual holds.
  // readSelection 按快照身份记忆，并在 isEqual 成立时保留上一个值。
  const readSelection = React.useMemo(() => {
    let memo: { snapshot: ConversationEventWindowControllerSnapshot; value: T } | null = null;
    return () => {
      const snapshot = store.getSnapshot();
      if (memo !== null && memo.snapshot === snapshot) return memo.value;
      const next = selector(snapshot);
      const value = memo !== null && isEqual(memo.value, next) ? memo.value : next;
      memo = { snapshot, value };
      return value;
    };
  }, [isEqual, selector, store]);
  return React.useSyncExternalStore(store.subscribe, readSelection, readSelection);
}

/** Compares two plain-data selections by structure, short-circuiting on shared references.
 * 按结构比较两个纯数据选择结果，遇到共享引用时直接短路。
 *
 * @param left previous selection built only from JSON-like values (no Map, Set, Date or class instances).
 *             仅由类 JSON 值构成（不含 Map、Set、Date 或类实例）的上一个选择结果。
 * @param right newly derived selection with the same constraint.
 *              满足同样约束的新派生选择结果。
 * @returns whether both selections hold equal data.
 *          两个选择结果是否持有相等数据。 */
export function conversationSelectionEqual(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true;
  if (typeof left !== "object" || typeof right !== "object" || left === null || right === null) return false;
  if (Array.isArray(left) !== Array.isArray(right)) return false;
  const leftRecord = left as Record<string, unknown>;
  const rightRecord = right as Record<string, unknown>;
  const leftKeys = Object.keys(leftRecord);
  if (leftKeys.length !== Object.keys(rightRecord).length) return false;
  return leftKeys.every((key) => Object.hasOwn(rightRecord, key) && conversationSelectionEqual(leftRecord[key], rightRecord[key]));
}

/** Reads the selected-session hydration lifecycle through the same external-store publication.
 * 通过同一外部 store 发布读取选中会话 hydration 生命周期。 */
export function useSelectedConversationEventWindowStatus():
SelectedConversationEventWindowStatus {
  const store = useSelectedConversationEventWindowExternalStore();
  return React.useSyncExternalStore(
    store.subscribe,
    store.getStatus,
    store.getStatus,
  );
}

/** Reads commands bound to the selected owner represented by the latest external-store snapshot.
 * 读取绑定到最近外部 store 快照所代表选中归属的命令。
 *
 * @returns an owner-bound command object that becomes invalid after owner replacement.
 *          在归属替换后失效的归属绑定命令对象。 */
export function useSelectedConversationEventWindowCommands():
SelectedConversationEventWindowCommands {
  const store = useSelectedConversationEventWindowExternalStore();
  // selectedSnapshot subscribes this command-only consumer to owner replacement publications.
  // selectedSnapshot 让仅消费命令的组件订阅归属替换发布。
  const selectedSnapshot = React.useSyncExternalStore(
    store.subscribe,
    store.getSnapshot,
    store.getServerSnapshot,
  );
  void selectedSnapshot;
  return store.getCommands();
}
