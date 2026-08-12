import React from "react";

import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  currentBrowserServiceConnectionStatus,
  type BrowserServiceConnectionDetail,
} from "@/service/browser-service-discovery";

// TerminalDisplayConnectionState describes whether display delivery is currently usable and its recovery generation.
// TerminalDisplayConnectionState 描述显示交付当前是否可用及其恢复代次。
export interface TerminalDisplayConnectionState {
  // connectionAvailable is false as soon as the browser event transport is interrupted.
  // connectionAvailable 在浏览器事件传输中断后立即为 false。
  connectionAvailable: boolean;
  // connectionEpoch advances after every interrupted-to-connected recovery.
  // connectionEpoch 在每次从中断恢复为已连接后递增。
  connectionEpoch: number;
}

// useTerminalDisplayConnectionState detaches hidden delivery during interruption and requests one fresh baseline after recovery.
// useTerminalDisplayConnectionState 在中断期间解除隐藏交付，并在恢复后请求一次新基线。
export function useTerminalDisplayConnectionState(): TerminalDisplayConnectionState {
  // connectionState mirrors the latest known transport state without polling.
  // connectionState 以无轮询方式镜像最近已知的传输状态。
  const [connectionState, setConnectionState] = React.useState<TerminalDisplayConnectionState>(() => {
    const currentStatus = currentBrowserServiceConnectionStatus();
    return {
      connectionAvailable: currentStatus !== "disconnected" && currentStatus !== "reconnecting",
      connectionEpoch: 0,
    };
  });
  React.useEffect(() => {
    let interrupted = !connectionState.connectionAvailable;
    // handleConnectionChange disables visible delivery immediately and advances once after recovery.
    // handleConnectionChange 立即停用可见交付，并在恢复后只推进一次。
    const handleConnectionChange = (event: Event) => {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (detail.status === "disconnected" || detail.status === "reconnecting") {
        interrupted = true;
        setConnectionState((current) => current.connectionAvailable
          ? { ...current, connectionAvailable: false }
          : current);
      } else if (detail.status === "connected" && interrupted) {
        interrupted = false;
        setConnectionState((current) => ({
          connectionAvailable: true,
          connectionEpoch: current.connectionEpoch + 1,
        }));
      } else if (detail.status === "connected") {
        setConnectionState((current) => current.connectionAvailable
          ? current
          : { ...current, connectionAvailable: true });
      }
    };
    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionChange);
    return () => window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionChange);
  }, [connectionState.connectionAvailable]);
  return connectionState;
}
