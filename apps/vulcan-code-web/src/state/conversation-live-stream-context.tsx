import React from "react";
import {
  ConversationLiveStreamStore,
  contextCompactionPreviewKey,
  providerRetryStatusKey,
  toolRequestProgressKey,
  toolRequestMembershipKey,
  transientReasoningStatusKey,
  type ProviderRetryCountdown,
} from "./conversation-live-stream-store";
import type { ToolRequestProgressSnapshot, TransientReasoningStatus } from "@/types";

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

// useConversationProviderRetryStatus subscribes to one conversation's pending Provider retry wait.
// useConversationProviderRetryStatus 订阅单个会话待执行的 Provider 重试等待。
// Parameters: conversationId is the exact conversation identity, or null to disable the subscription.
// 参数：conversationId 是精确会话身份；为 null 时禁用订阅。
// Returns: the latest locally anchored countdown, or null when no retry is waiting.
// 返回值：最新的本地锚定倒计时；没有等待中的重试时返回 null。
export function useConversationProviderRetryStatus(
  conversationId: string | null,
): ProviderRetryCountdown | null {
  const store = useConversationLiveStreamStore();
  const key = conversationId ? providerRetryStatusKey(conversationId) : null;
  const subscribe = React.useCallback(
    (listener: () => void) => (key ? store.subscribe(key, listener) : () => {}),
    [key, store],
  );
  const getSnapshot = React.useCallback(
    () => (conversationId ? store.getProviderRetryStatus(conversationId) : null),
    [conversationId, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// ConversationTailStatusKind names which transcript-tail status line is showing; a retry wait outranks reasoning.
// ConversationTailStatusKind 指明当前显示哪一条转录尾部状态行；重试等待优先于思考状态。
export type ConversationTailStatusKind = "providerRetry" | "transientReasoning";

// useConversationTailStatusKind reports only which tail status is present, never its changing content.
// useConversationTailStatusKind 只报告存在哪种尾部状态，从不报告其变化中的内容。
// Parameters: conversationId is the exact conversation identity, or null to disable the subscription.
// 参数：conversationId 是精确会话身份；为 null 时禁用订阅。
// Returns: the present status kind or null; token-count updates keep the same value, so callers do not re-render.
// 返回值：存在的状态类型或 null；token 计数更新保持相同值，因此调用方不会重新渲染。
export function useConversationTailStatusKind(
  conversationId: string | null,
): ConversationTailStatusKind | null {
  const store = useConversationLiveStreamStore();
  const subscribe = React.useCallback(
    (listener: () => void) => {
      if (!conversationId) return () => {};
      const unsubscribeRetry = store.subscribe(providerRetryStatusKey(conversationId), listener);
      const unsubscribeReasoning = store.subscribe(transientReasoningStatusKey(conversationId), listener);
      return () => {
        unsubscribeRetry();
        unsubscribeReasoning();
      };
    },
    [conversationId, store],
  );
  const getSnapshot = React.useCallback((): ConversationTailStatusKind | null => {
    if (!conversationId) return null;
    if (store.getProviderRetryStatus(conversationId)) return "providerRetry";
    return store.getTransientReasoningStatus(conversationId) ? "transientReasoning" : null;
  }, [conversationId, store]);
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// useConversationContextCompactionPreview subscribes one compaction card to its live summary preview.
// useConversationContextCompactionPreview 让单张压缩卡订阅其实时摘要预览。
// Parameters: conversationId and apiCallId identify the card; null for either disables the subscription.
// 参数：conversationId 与 apiCallId 标识卡片；任一为 null 时禁用订阅。
// Returns: the latest preview text, or null when absent or cleared.
// 返回值：最新预览文本；不存在或已清除时返回 null。
export function useConversationContextCompactionPreview(
  conversationId: string | null,
  apiCallId: number | null,
): string | null {
  const store = useConversationLiveStreamStore();
  const key = conversationId && apiCallId !== null ? contextCompactionPreviewKey(conversationId, apiCallId) : null;
  const subscribe = React.useCallback(
    (listener: () => void) => (key ? store.subscribe(key, listener) : () => {}),
    [key, store],
  );
  const getSnapshot = React.useCallback(
    () => (conversationId && apiCallId !== null ? store.getContextCompactionPreview(conversationId, apiCallId) : null),
    [apiCallId, conversationId, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// useConversationToolRequestProgress subscribes one tool card to one ephemeral request stream.
// useConversationToolRequestProgress 让单张工具卡订阅单个瞬态请求流。
// Parameters: conversationId owns the request and requestStreamId is the exact coordinator identity.
// 参数：conversationId 持有请求，requestStreamId 是精确协调器身份。
// Returns: the latest safe formation snapshot, or null after terminal clearing.
// 返回值：最新安全构造快照；终态清除后返回空。
export function useConversationToolRequestProgress(
  conversationId: string | null,
  requestStreamId: string | null,
): ToolRequestProgressSnapshot | null {
  // store owns the high-frequency value outside global React workbench state.
  // store 在全局 React 工作台状态之外持有高频值。
  const store = useConversationLiveStreamStore();
  // key isolates the exact tool card subscription from every other active request.
  // key 将当前精确工具卡订阅与其他全部活动请求隔离。
  const key = conversationId && requestStreamId
    ? toolRequestProgressKey(conversationId, requestStreamId)
    : null;
  // subscribe connects React only to the matching request identity.
  // subscribe 仅将 React 连接到匹配的请求身份。
  const subscribe = React.useCallback(
    (listener: () => void) => (key ? store.subscribe(key, listener) : () => {}),
    [key, store],
  );
  // getSnapshot returns the stable Store object without cloning on reads.
  // getSnapshot 返回稳定 Store 对象，读取时不会克隆。
  const getSnapshot = React.useCallback(
    () => (conversationId && requestStreamId
      ? store.getToolRequestProgress(conversationId, requestStreamId)
      : null),
    [conversationId, requestStreamId, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

// Subscribes to draft membership for one conversation; returns stable seeds, not token updates.
// 订阅单个会话的草稿成员；返回稳定种子，不订阅 Token 更新。
export function useConversationToolRequestMembership(
  conversationId: string,
): readonly ToolRequestProgressSnapshot[] {
  // The mounted store owns both membership and leaf progress.
  // 已挂载存储同时持有成员和叶子进度。
  const store = useConversationLiveStreamStore();
  // Membership changes only when a request starts or leaves formation.
  // 成员仅在请求开始或离开构造阶段时变化。
  const subscribe = React.useCallback(
    (listener: () => void) => store.subscribe(toolRequestMembershipKey(conversationId), listener),
    [conversationId, store],
  );
  // Stable seeds keep high-frequency arguments outside branch projection.
  // 稳定种子让高频参数留在分支投影之外。
  const getSnapshot = React.useCallback(
    () => store.getToolRequestMembership(conversationId), [conversationId, store],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
