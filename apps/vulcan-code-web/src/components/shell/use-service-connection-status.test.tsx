// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { emitBrowserServiceConnectionStatus } from "@/service/browser-service-discovery";
import { useServiceConnectionStatus } from "./use-service-connection-status";

// ConnectionProbe renders the hook value so the test reads what a status bar would read.
// ConnectionProbe 渲染 hook 的值，使测试读取到状态栏所读取的内容。
function ConnectionProbe() {
  return <span>{useServiceConnectionStatus() ?? "none"}</span>;
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
});
