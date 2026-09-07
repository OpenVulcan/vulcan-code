// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import type { VceJobSummary, VceProjectChangedEvent } from "@/types";

import {
  closeWebSocketWhenReady,
  waitForWebSocketOpen,
  BrowserServiceTransport,
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

describe("VCE browser event transport", () => {
  // Use production subscription and message dispatch while replacing only browser network primitives.
  // 使用生产订阅和消息分发，仅替换浏览器网络原语。
  it("routes both semantic topics through one socket and retains the remaining subscriber", async () => {
    // Exact constructed sockets expose the production channel without any real network connection.
    // 精确构造的套接字暴露生产通道，不建立任何真实网络连接。
    const sockets: BrowserSocketFixture[] = [];
    // Reject accidental HTTP discovery so this fixture cannot contact a user service.
    // 拒绝意外的 HTTP 发现，确保此夹具不能联系用户服务。
    const fetchSpy = vi.fn(() => { throw new Error("Unexpected browser fixture HTTP request"); });
    vi.stubGlobal("fetch", fetchSpy);
    vi.stubGlobal("WebSocket", class extends BrowserSocketFixture {
      // Capture the socket created by the real transport; no inputs or network effects are consumed.
      // 捕获真实传输创建的套接字；不消费输入，也不产生网络副作用。
      constructor() {
        super();
        sockets.push(this);
      }
    });
    // An explicit endpoint prevents discovery and belongs only to this disconnected fixture.
    // 明确端点阻止发现，并且仅属于此断网夹具。
    const transport = new BrowserServiceTransport("http://127.0.0.1:17999");
    try {
      // Independent handlers must receive only their own exact typed payload.
      // 独立处理器必须仅接收自己的精确强类型载荷。
      const projectHandler = vi.fn();
      const jobHandler = vi.fn();
      // Both subscriptions stay pending until the single physical channel opens.
      // 两项订阅在唯一物理通道打开前都保持等待。
      const projectSubscription = transport.subscribe<VceProjectChangedEvent>("vce.project.changed", projectHandler);
      const jobSubscription = transport.subscribe<VceJobSummary>("vce.job.changed", jobHandler);
      await vi.waitFor(() => expect(sockets).toHaveLength(1));
      // The length assertion proves the exact socket exists before it is used.
      // 长度断言证明使用前精确套接字已经存在。
      const socket = sockets[0]!;
      socket.open();
      // Retain independent ownership tokens from the real transport.
      // 保留真实传输返回的独立所有权令牌。
      const [unsubscribeProject, unsubscribeJob] = await Promise.all([projectSubscription, jobSubscription]);
      // Exact lifecycle metadata is distinct from job progress and never inferred from it.
      // 精确生命周期元数据独立于作业进度，绝不由进度推断。
      const project: VceProjectChangedEvent = {
        projectId: "semantic-project", settingsRevision: 3, lifecycle: "ready", checkpointVersion: 7,
      };
      // A real typed receipt preserves its owner, identity, sequence, and old published checkpoint.
      // 真实强类型回执保留其所有者、身份、序号及旧的已发布检查点。
      const job: VceJobSummary = {
        projectId: project.projectId, jobId: "semantic-job", kind: "synchronize", state: "running",
        sequence: 4, createdAtUnixMs: 1000, updatedAtUnixMs: 1001,
        sourceCheckpointVersion: 7, resultCheckpointVersion: null,
        progress: { phase: "embedding", operationId: "semantic-operation", unit: "batches", completed: 1, total: 3, currentPath: null },
        followupRequested: false, error: null,
      };
      socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "vceProjectChanged", payload: project }) }));
      socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "vceJobChanged", payload: job }) }));
      expect(projectHandler).toHaveBeenCalledExactlyOnceWith(project);
      expect(jobHandler).toHaveBeenCalledExactlyOnceWith(job);
      unsubscribeProject();
      expect(socket.closeCalls).toBe(0);
      socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "vceProjectChanged", payload: project }) }));
      socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ kind: "vceJobChanged", payload: { ...job, sequence: 5 } }) }));
      expect(projectHandler).toHaveBeenCalledTimes(1);
      expect(jobHandler).toHaveBeenLastCalledWith({ ...job, sequence: 5 });
      unsubscribeJob();
      expect(socket.closeCalls).toBe(1);
      expect(fetchSpy).not.toHaveBeenCalled();
    } finally {
      transport.dispose();
      vi.unstubAllGlobals();
    }
  });
});
