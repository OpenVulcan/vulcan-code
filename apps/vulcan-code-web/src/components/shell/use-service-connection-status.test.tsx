// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emitBrowserServiceConnectionStatus } from "@/service/browser-service-discovery";
import { useServiceConnectionStatus, useServiceReconnected } from "./use-service-connection-status";

// ConnectionProbe renders the hook value so the test reads what a status bar would read.
// ConnectionProbe 渲染 hook 的值，使测试读取到状态栏所读取的内容。
function ConnectionProbe() {
  return <span>{useServiceConnectionStatus() ?? "none"}</span>;
}

// ReconnectProbe registers one recovery callback, as a view holding a failed read would.
// ReconnectProbe 注册一个恢复回调，如同持有失败读取的视图。
function ReconnectProbe(props: { onReconnected: () => void }) {
  useServiceReconnected(props.onReconnected);
  return null;
}

describe("useServiceConnectionStatus", () => {
  let root: Root;
  let host: HTMLDivElement;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

  // A dropped connection must surface immediately and clear once the socket reopens.
  // 连接中断必须立即呈现，并在套接字重新打开后清除。
  it("follows every connection event, including recovery", async () => {
    await act(async () => root.render(<ConnectionProbe />));
    await act(async () => emitBrowserServiceConnectionStatus("connected"));
    expect(host.textContent).toBe("connected");
    await act(async () => emitBrowserServiceConnectionStatus("reconnecting"));
    expect(host.textContent).toBe("reconnecting");
    await act(async () => emitBrowserServiceConnectionStatus("connected"));
    expect(host.textContent).toBe("connected");
  });

  // Recovery fires once per interruption, never for a connected event that follows another connected event.
  // 恢复回调每次中断只触发一次，已连接之后的已连接事件不会触发。
  it("reports each recovery from an interruption exactly once", async () => {
    const onReconnected = vi.fn();
    await act(async () => emitBrowserServiceConnectionStatus("connected"));
    await act(async () => root.render(<ReconnectProbe onReconnected={onReconnected} />));
    await act(async () => emitBrowserServiceConnectionStatus("connected", "duplicate"));
    expect(onReconnected).not.toHaveBeenCalled();
    await act(async () => emitBrowserServiceConnectionStatus("reconnecting"));
    await act(async () => emitBrowserServiceConnectionStatus("disconnected"));
    await act(async () => emitBrowserServiceConnectionStatus("connected"));
    await act(async () => emitBrowserServiceConnectionStatus("connected", "duplicate"));
    expect(onReconnected).toHaveBeenCalledOnce();
  });
});
