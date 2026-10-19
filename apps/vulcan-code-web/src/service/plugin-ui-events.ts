import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { PluginUiInvalidatedEvent } from "@/types";
import { invalidatePluginQueries } from "@/service/plugin-ui-query-runtime";
import { BROWSER_SERVICE_CONNECTION_EVENT, type BrowserServiceConnectionDetail } from "@/service/browser-service-discovery";

// PluginUiInvalidationListener receives a newer monotonic revision for one plugin-local topic.
// PluginUiInvalidationListener 接收单个插件内主题的更高单调修订号。
export type PluginUiInvalidationListener = (revision: number) => void;

// pluginUiInvalidationListeners stores mounted UI listeners by authenticated plugin and topic key.
// pluginUiInvalidationListeners 按已认证插件与主题键保存已挂载界面监听器。
const pluginUiInvalidationListeners = new Map<string, Set<PluginUiInvalidationListener>>();

// pluginUiLatestRevisions stores the highest observed revision for visibility recovery.
// pluginUiLatestRevisions 保存已观测到的最高修订号以支持可见性恢复。
const pluginUiLatestRevisions = new Map<string, number>();

// A restarted service may begin revisions at zero; previous process cursors must not suppress it.
// 重启服务可从零开始修订号，旧进程游标不能屏蔽新事件。
if (typeof window !== "undefined") window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, event => {
  if ((event as CustomEvent<BrowserServiceConnectionDetail>).detail.status !== "connected") pluginUiLatestRevisions.clear();
});

// pluginUiBackendSubscription stores the singleton backend event subscription task.
// pluginUiBackendSubscription 保存单例后端事件订阅任务。
let pluginUiBackendSubscription: Promise<() => void> | null = null;

// pluginUiEventKey builds one collision-free listener key from validated string parts.
// pluginUiEventKey 根据已校验字符串部分构建无冲突监听器键。
//
// Parameters: pluginId is the authenticated owner and topic is plugin-local.
// 参数：pluginId 是已认证所有者，topic 属于插件内部。
// Returns: one length-delimited listener key.
// 返回值：一个长度分隔的监听器键。
function pluginUiEventKey(pluginId: string, topic: string): string {
  return `${pluginId.length}:${pluginId}${topic}`;
}

// dispatchPluginUiInvalidation applies monotonic ordering before notifying mounted listeners.
// dispatchPluginUiInvalidation 在通知已挂载监听器前应用单调顺序。
//
// Parameters: event is the exact revision-only service payload.
// 参数：event 是精确的仅含修订号服务载荷。
// Returns: no value.
// 返回值：无返回值。
function dispatchPluginUiInvalidation(event: PluginUiInvalidatedEvent): void {
  const key = pluginUiEventKey(event.pluginId, event.topic);
  const currentRevision = pluginUiLatestRevisions.get(key) ?? 0;
  if (!Number.isSafeInteger(event.revision) || event.revision <= currentRevision) {
    return;
  }
  pluginUiLatestRevisions.set(key, event.revision);
  invalidatePluginQueries(event.pluginId);
  const listeners = pluginUiInvalidationListeners.get(key);
  if (!listeners) {
    return;
  }
  for (const listener of listeners) {
    listener(event.revision);
  }
}

// ensurePluginUiBackendSubscription starts the shared backend subscription exactly once.
// ensurePluginUiBackendSubscription 恰好一次启动共享后端订阅。
//
// Returns: no value; transport failures leave polling fallback active in plugin pages.
// 返回值：无返回值；传输失败时插件页面继续使用轮询降级。
function ensurePluginUiBackendSubscription(): void {
  // The browser preview has no backend, so mounted pages keep polling their fixtures.
  // 浏览器预览没有后端，已挂载页面继续轮询其示例数据。
  if (pluginUiBackendSubscription !== null || !hasServiceRuntime()) {
    return;
  }
  pluginUiBackendSubscription = serviceClient
    .subscribe<PluginUiInvalidatedEvent>("plugin.ui.invalidated", dispatchPluginUiInvalidation)
    .catch((error: unknown) => {
      pluginUiBackendSubscription = null;
      console.warn("Plugin UI invalidation subscription is unavailable; mounted pages will poll.", error);
      return () => undefined;
    });
}

// subscribePluginUiInvalidation registers one mounted plugin listener and returns synchronous cleanup.
// subscribePluginUiInvalidation 注册一个已挂载插件监听器并返回同步清理函数。
//
// Parameters: pluginId is the authenticated owner, topic is plugin-local, and listener receives revisions.
// 参数：pluginId 是已认证所有者，topic 属于插件内部，listener 接收修订号。
// Returns: an idempotent listener cleanup function.
// 返回值：一个幂等监听器清理函数。
export function subscribePluginUiInvalidation(
  pluginId: string,
  topic: string,
  listener: PluginUiInvalidationListener,
): () => void {
  ensurePluginUiBackendSubscription();
  const key = pluginUiEventKey(pluginId, topic);
  const listeners = pluginUiInvalidationListeners.get(key) ?? new Set<PluginUiInvalidationListener>();
  listeners.add(listener);
  pluginUiInvalidationListeners.set(key, listeners);
  let active = true;
  return () => {
    if (!active) {
      return;
    }
    active = false;
    listeners.delete(listener);
    if (listeners.size === 0) {
      pluginUiInvalidationListeners.delete(key);
    }
  };
}

// latestPluginUiInvalidationRevision returns the highest revision already observed for one topic.
// latestPluginUiInvalidationRevision 返回单个主题已观测到的最高修订号。
//
// Parameters: pluginId is the authenticated owner and topic is plugin-local.
// 参数：pluginId 是已认证所有者，topic 属于插件内部。
// Returns: the latest revision or zero before any event.
// 返回值：最新修订号；尚无事件时返回零。
export function latestPluginUiInvalidationRevision(pluginId: string, topic: string): number {
  return pluginUiLatestRevisions.get(pluginUiEventKey(pluginId, topic)) ?? 0;
}
