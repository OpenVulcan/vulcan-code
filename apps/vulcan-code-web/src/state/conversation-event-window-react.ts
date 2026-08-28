import React from "react";

import type {
  ConversationEventWindowController,
  ConversationEventWindowControllerListener,
  ConversationEventWindowControllerSnapshot,
} from "@/service/conversation-event-window-controller";
import type {
  SelectedConversationEventWindowCommands,
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
}

/** React owner boundary for the selected-session V1 external store.
 * 选中会话 V1 外部 store 的 React 归属边界。 */
const ConversationEventWindowExternalStoreContext =
  React.createContext<ConversationEventWindowExternalStore | null>(null);

/** Creates one React external-store adapter around a V1 window controller.
 * 围绕 V1 窗口控制器创建一个 React 外部 store 适配器。
 *
 * @param controller controller that owns the canonical event window.
 *                   持有 canonical 事件窗口的控制器。
 * @returns stable external-store callbacks bound to that controller.
 *          绑定到该控制器的稳定外部 store 回调。 */
export function createConversationEventWindowExternalStore(
  controller: ConversationEventWindowOwnedStore,
): ConversationEventWindowExternalStore {
  return {
    subscribe: (listener) => controller.subscribe(listener),
    getSnapshot: () => controller.snapshot(),
    getServerSnapshot: () => controller.snapshot(),
    getCommands: () => controller.commands(),
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
  // readableStore changes only when controller ownership changes, preserving callback identities across renders.
  // readableStore 仅在控制器归属变化时改变，从而跨渲染保持回调身份稳定。
  const readableStore = React.useMemo(
    () => ({
      subscribe: (listener: ConversationEventWindowControllerListener) =>
        controller.subscribe(listener),
      getSnapshot: () => controller.snapshot(),
      getServerSnapshot: () => controller.snapshot(),
    }),
    [controller],
  );
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
