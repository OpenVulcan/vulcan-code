// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { readElectronServiceConnection } from "@/host-shell/electron-bridge";
import { BrowserServiceTransport } from "./browserTransport";
import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  emitBrowserServiceConnectionStatus,
  type BrowserServiceConnectionDetail,
} from "./browser-service-discovery";

vi.mock("@/host-shell/electron-bridge", () => ({ readElectronServiceConnection: vi.fn() }));

/** Controls physical socket readiness independently of successful HTTP requests.
 * 独立于成功的 HTTP 请求控制物理套接字就绪状态。 */
class RecoverySocket extends EventTarget {
  /** Browser state constants and the currently simulated handshake state.
   * 浏览器状态常量与当前模拟握手状态。 */
  readonly CONNECTING = 0;
  readonly OPEN = 1;
  readonly CLOSED = 3;
  readyState = this.CONNECTING;

  /** Completes this exact handshake and notifies the real transport; returns nothing.
   * 完成本次精确握手并通知真实传输；无返回值。 */
  open(): void {
    this.readyState = this.OPEN;
    this.dispatchEvent(new Event("open"));
  }

  /** Simulates the server terminating an overflowing connection; returns nothing.
   * 模拟服务端终止溢出连接；无返回值。 */
  close(): void {
    this.readyState = this.CLOSED;
    this.dispatchEvent(new Event("close"));
  }
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});

it("recovers completion after the replacement socket opens, not after discovery or an unrelated RPC", async () => {
  vi.useFakeTimers();
  // Isolated storage avoids Node's optional native localStorage replacing jsdom storage.
  // 隔离存储避免 Node 可选原生 localStorage 覆盖 jsdom 存储。
  vi.stubGlobal("localStorage", { getItem: vi.fn(), setItem: vi.fn(), removeItem: vi.fn() });
  vi.stubGlobal("sessionStorage", { getItem: vi.fn(), setItem: vi.fn(), removeItem: vi.fn() });
  emitBrowserServiceConnectionStatus("disconnected");
  // Socket instances are the only mocked network event primitive; discovery and dispatch remain real.
  // 套接字实例是唯一模拟的网络事件原语；发现与分发仍使用真实代码。
  const sockets: RecoverySocket[] = [];
  vi.stubGlobal("WebSocket", class extends RecoverySocket {
    /** Captures each physical connection without contacting the user's service.
     * 捕获每条物理连接，不联系用户服务。 */
    constructor() { super(); sockets.push(this); }
  });
  vi.mocked(readElectronServiceConnection).mockResolvedValue({
    baseUrl: "http://127.0.0.1:17921", pid: 42, startedAt: "123",
    serviceToken: "recovery-fixture", status: "ready",
  });
  // The backend completes while no socket can deliver its terminal event.
  // 后台在没有套接字可以传递终态事件时完成任务。
  let serverState = "editing +51";
  vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ok: true, result: serverState }))));
  // Recovery follows the same one-disconnect/one-connected gate as the workbench owner.
  // 恢复遵循与工作台归属方相同的一次断线、一次 connected 门控。
  const transport = new BrowserServiceTransport();
  const recovered: string[] = [];
  const statuses: string[] = [];
  let interrupted = false;
  const onConnection = (event: Event): void => {
    const { status } = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
    statuses.push(status);
    if (status !== "connected") { interrupted = true; return; }
    if (!interrupted) return;
    interrupted = false;
    void transport.request<string>("fixture_recover").then((state) => recovered.push(state));
  };
  window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, onConnection);
  try {
    const subscription = transport.subscribe("conversation.event.delta.v1", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(sockets).toHaveLength(1);
    expect(statuses).toEqual([]);
    sockets[0]!.open();
    await subscription;
    sockets[0]!.close();
    await vi.advanceTimersByTimeAsync(500);
    expect(sockets).toHaveLength(2);
    expect(sockets[1]!.readyState).toBe(sockets[1]!.CONNECTING);
    await transport.request("fixture_unrelated_http_success");
    expect(statuses).toEqual(["connected", "disconnected", "reconnecting"]);
    expect(recovered).toEqual([]);
    serverState = "completed";
    sockets[1]!.open();
    await vi.advanceTimersByTimeAsync(0);
    expect(recovered).toEqual(["completed"]);
    expect(statuses.at(-1)).toBe("connected");
  } finally {
    transport.dispose();
    window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, onConnection);
  }
});
