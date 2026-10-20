import React from "react";
import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  currentBrowserServiceConnectionStatus,
  type BrowserServiceConnectionStatus,
} from "@/service/browser-service-discovery";

// subscribeServiceConnection forwards every local-service connection event to React.
// subscribeServiceConnection 将每次本地服务连接事件转发给 React。
function subscribeServiceConnection(onChange: () => void): () => void {
  window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, onChange);
  return () => window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, onChange);
}

// useServiceConnectionStatus returns the latest local-service connection state, or null before the first event.
// useServiceConnectionStatus 返回最近的本地服务连接状态；首个事件之前为 null。
export function useServiceConnectionStatus(): BrowserServiceConnectionStatus | null {
  return React.useSyncExternalStore(subscribeServiceConnection, currentBrowserServiceConnectionStatus, () => null);
}
