// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import { openCoreBrowserStream } from "./core-browser-service";

vi.mock("@/service/webui-service-runtime", () => ({ serviceClient: { request: vi.fn(async () => ({ ticket: "ticket" })) } }));
vi.mock("@/service/browser-service-discovery", () => ({
  resolveBrowserServiceBaseUrl: () => "http://127.0.0.1:17600",
  resolveBrowserServiceAccessToken: () => null,
}));

// Snapshot and resume acknowledge an open stream and must never look like a new connection attempt.
// snapshot 与 resume 确认画面流已打开，绝不能表现为重新连接。
it("keeps snapshot and resumed streams connected while reporting real errors separately", async () => {
  // socket records listeners without opening any network connection.
  // socket 记录监听器，不建立网络连接。
  const socket = new EventTarget();
  // Socket provides the constructor used by the service; returns this test's event target.
  // Socket 提供服务使用的构造器；返回本测试的事件目标。
  const Socket = vi.fn(function () { return socket; });
  vi.stubGlobal("WebSocket", Socket);
  const callbacks = { onFrame: vi.fn(), onState: vi.fn(), onError: vi.fn() };
  try {
    await openCoreBrowserStream("owner", "browser", "page", callbacks);
    socket.dispatchEvent(new Event("open"));
    for (const mode of ["snapshot", "resume"]) {
      socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "ready", mode }) }));
    }
    expect(callbacks.onState.mock.calls).toEqual([["connected"], ["connected"], ["connected"]]);
    socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "error", message: "stale_generation" }) }));
    expect(callbacks.onError).toHaveBeenCalledWith("stale_generation");
    socket.dispatchEvent(new Event("close"));
    expect(callbacks.onState).toHaveBeenLastCalledWith("disconnected");
  } finally {
    vi.unstubAllGlobals();
  }
});
