import { describe, expect, it } from "vitest";

import {
  closeWebSocketWhenReady,
  waitForWebSocketOpen,
} from "@/service/browserTransport";

// BrowserSocketFixture models the WebSocket states required by close lifecycle tests.
// BrowserSocketFixture 模拟关闭生命周期测试所需的 WebSocket 状态。
class BrowserSocketFixture extends EventTarget {
  // CONNECTING mirrors the browser WebSocket connecting state.
  // CONNECTING 对应浏览器 WebSocket 的建连状态。
  readonly CONNECTING = 0;

  // OPEN mirrors the browser WebSocket open state.
  // OPEN 对应浏览器 WebSocket 的已连接状态。
  readonly OPEN = 1;

  // CLOSING mirrors the browser WebSocket closing state.
  // CLOSING 对应浏览器 WebSocket 的关闭中状态。
  readonly CLOSING = 2;

  // CLOSED mirrors the browser WebSocket closed state.
  // CLOSED 对应浏览器 WebSocket 的已关闭状态。
  readonly CLOSED = 3;

  // readyState stores the current simulated connection state.
  // readyState 保存当前模拟连接状态。
  readyState = this.CONNECTING;

  // closeCalls records how many close requests reached the socket.
  // closeCalls 记录到达套接字的关闭请求次数。
  closeCalls = 0;

  // close records one valid close request.
  // close 记录一次有效关闭请求。
  close(): void {
    this.closeCalls += 1;
    this.readyState = this.CLOSING;
  }

  // open completes the handshake and emits the browser open event.
  // open 完成握手并触发浏览器 open 事件。
  open(): void {
    this.readyState = this.OPEN;
    this.dispatchEvent(new Event("open"));
  }

  // fail rejects the simulated opening handshake through the browser error event.
  // fail 通过浏览器 error 事件拒绝模拟的建连握手。
  fail(): void {
    this.dispatchEvent(new Event("error"));
  }
}

describe("closeWebSocketWhenReady", () => {
  // Verifies an unsubscribe never calls close while the WebSocket handshake is incomplete.
  // 验证取消订阅不会在 WebSocket 握手完成前调用关闭。
  it("defers close until a connecting socket opens", () => {
    const socket = new BrowserSocketFixture();
    closeWebSocketWhenReady(socket as unknown as WebSocket, () => true);

    expect(socket.closeCalls).toBe(0);
    socket.open();
    expect(socket.closeCalls).toBe(1);
  });

  // Verifies a new subscriber can retain the pending socket before the handshake completes.
  // 验证新订阅者可在握手完成前保留待连接套接字。
  it("revalidates the close condition after opening", () => {
    const socket = new BrowserSocketFixture();
    let shouldClose = true;
    closeWebSocketWhenReady(socket as unknown as WebSocket, () => shouldClose);

    shouldClose = false;
    socket.open();
    expect(socket.closeCalls).toBe(0);
  });
});

describe("waitForWebSocketOpen", () => {
  // Verifies event subscriptions cannot report readiness while the socket is still connecting.
  // 验证事件订阅不会在套接字仍处于建连状态时报告就绪。
  it("waits for the opening handshake", async () => {
    const socket = new BrowserSocketFixture();
    let resolved = false;
    const readiness = waitForWebSocketOpen(socket as unknown as WebSocket).then(() => {
      resolved = true;
    });

    await Promise.resolve();
    expect(resolved).toBe(false);
    socket.open();
    await readiness;
    expect(resolved).toBe(true);
  });

  // Verifies a failed handshake rejects bootstrap instead of exposing an event-blind workbench.
  // 验证失败握手会拒绝启动，而不是暴露无法接收事件的工作台。
  it("rejects a failed opening handshake", async () => {
    const socket = new BrowserSocketFixture();
    const readiness = waitForWebSocketOpen(socket as unknown as WebSocket);

    socket.fail();
    await expect(readiness).rejects.toThrow("connection failed");
  });
});
