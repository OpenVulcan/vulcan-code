import React from "react";
import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  currentBrowserServiceConnectionStatus,
  type BrowserServiceConnectionDetail,
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

// useServiceReconnected calls onReconnected once each time an interrupted local-service connection reopens,
// so views holding a read error from the outage can retry without a manual refresh.
// useServiceReconnected 在中断的本地服务连接每次重新打开时调用一次 onReconnected，
// 使持有断线期间读取错误的视图无需手动刷新即可重试。
export function useServiceReconnected(onReconnected: () => void): void {
  const callback = React.useRef(onReconnected);
  callback.current = onReconnected;
  React.useEffect(() => {
    const initial = currentBrowserServiceConnectionStatus();
    let interrupted = initial === "disconnected" || initial === "reconnecting";
    const handle = (event: Event) => {
      const status = (event as CustomEvent<BrowserServiceConnectionDetail>).detail?.status;
      if (status === "disconnected" || status === "reconnecting") {
        interrupted = true;
      } else if (status === "connected" && interrupted) {
        interrupted = false;
        callback.current();
      }
    };
    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handle);
    return () => window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handle);
  }, []);
}
