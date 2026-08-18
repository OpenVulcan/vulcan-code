import React from "react";
import {
  ConversationLiveStreamStore,
  type ConversationLiveStreamRevision,
  transientReasoningStatusKey,
} from "./conversation-live-stream-store";
import type { TransientReasoningStatus } from "@/types";

// ConversationLiveStreamContext carries one stable Store identity through the Workbench tree.
// ConversationLiveStreamContext 在 Workbench 树中传递单个稳定 Store 身份。
const ConversationLiveStreamContext = React.createContext<ConversationLiveStreamStore | null>(null);

// ConversationLiveStreamProvider exposes a stable external Store without propagating body updates through React props.
// ConversationLiveStreamProvider 暴露稳定外部 Store，避免正文更新沿 React props 传播。
// Parameters: props carries the Workbench-owned Store and its consumer subtree.
// 参数：props 携带 Workbench 持有的 Store 及其消费子树。
// Returns: one context provider element with stable Store identity.
// 返回值：一个 Store 身份稳定的上下文 Provider 元素。
export function ConversationLiveStreamProvider(props: {
  // Store instance owned by WorkbenchApp for the complete WebUI lifetime.
  // WorkbenchApp 在整个 WebUI 生命周期持有的 Store 实例。
  store: ConversationLiveStreamStore;
  // Workbench subtree allowed to consume live streams.
  // 可消费实时流的 Workbench 子树。
  children: React.ReactNode;
}) {
  return (
    <ConversationLiveStreamContext.Provider value={props.store}>
      {props.children}
    </ConversationLiveStreamContext.Provider>
  );
}

// useConversationLiveStreamStore returns the exact Store owned by the mounted Workbench.
// useConversationLiveStreamStore 返回当前挂载 Workbench 持有的精确 Store。
// Returns: the required Store instance from the nearest provider.
// 返回值：最近 Provider 提供的必需 Store 实例。
export function useConversationLiveStreamStore(): ConversationLiveStreamStore {
  // store is required because live stream consumers must never silently fall back to component-local text.
  // store 为必需项，因为实时流消费者绝不能静默回退到组件局部文本。
  const store = React.useContext(ConversationLiveStreamContext);
  if (!store) {
    throw new Error("ConversationLiveStreamProvider is missing");
  }
  return store;
}

// useConversationLiveStreamRevision subscribes one component to one exact streamed message.
// useConversationLiveStreamRevision 让单个组件订阅一条精确流式消息。
// Parameters: key is the exact compound stream identity, or null to disable the subscription.
// 参数：key 是精确复合流身份；为 null 时禁用订阅。
// Returns: the latest immutable revision, or null when the keyed stream does not exist.
// 返回值：最新不可变 revision；对应流不存在时返回 null。
export function useConversationLiveStreamRevision(
  key: string | null,
): ConversationLiveStreamRevision | null {
  // store is stable for the complete Workbench mount.
  // store 在整个 Workbench 挂载周期保持稳定。
  const store = useConversationLiveStreamStore();
  // subscribe binds React only to the selected stream key.
  // subscribe 只把 React 绑定到选定流身份。
  const subscribe = React.useCallback(
    (listener: () => void) => (key ? store.subscribe(key, listener) : () => {}),
    [key, store],
  );
  // getSnapshot returns an immutable revision object and never materializes the full body.
  // getSnapshot 返回不可变版本对象，且绝不物化完整正文。
  const getSnapshot = React.useCallback(
    () => (key ? store.getRevision(key) : null),
    [key, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// useConversationTransientReasoningStatus subscribes to one ephemeral reasoning status collection.
// useConversationTransientReasoningStatus 订阅单个瞬态推理状态集合。
// Parameters: conversationId is the exact conversation identity, or null to disable the subscription.
// 参数：conversationId 是精确会话身份；为 null 时禁用订阅。
// Returns: the latest replace-in-place status, or null after it is cleared.
// 返回值：最新原地替换式状态；清除后返回 null。
export function useConversationTransientReasoningStatus(
  conversationId: string | null,
): TransientReasoningStatus | null {
  // store owns the request-local value outside the global React snapshot.
  // store 在全局 React 快照之外持有请求局部值。
  const store = useConversationLiveStreamStore();
  // key isolates the exact conversation status subscription.
  // key 隔离精确的会话状态订阅。
  const key = conversationId ? transientReasoningStatusKey(conversationId) : null;
  // subscribe attaches React only to this request-local status key.
  // subscribe 仅将 React 连接到当前请求局部状态键。
  const subscribe = React.useCallback(
    (listener: () => void) => (key ? store.subscribe(key, listener) : () => {}),
    [key, store],
  );
  // getSnapshot reads the latest replacement value synchronously.
  // getSnapshot 同步读取最新替换值。
  const getSnapshot = React.useCallback(
    () => (conversationId ? store.getTransientReasoningStatus(conversationId) : null),
    [conversationId, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
