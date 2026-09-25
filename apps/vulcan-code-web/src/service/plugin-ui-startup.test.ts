import { expect, it, vi } from "vitest";
import type { StartupSnapshot } from "@/types";
import { waitForPluginStartup, type PluginStartupSource } from "./plugin-ui-startup";

// Build controlled startup transport; emit drives real readiness without timer polling.
// 构造可控启动传输；emit 驱动真实就绪状态，不使用定时轮询。
function startupFixture() {
  // A pending snapshot must not authorize plugin module loading.
  // 未就绪快照不能允许插件模块加载。
  const pending: StartupSnapshot = { ready: false, phase: "workspace", steps: [], recentLogs: [] };
  const ready: StartupSnapshot = { ...pending, ready: true, phase: "ready" };
  const unsubscribe = vi.fn();
  let listener!: (snapshot: StartupSnapshot) => void;
  const source: PluginStartupSource = {
    read: vi.fn(async () => pending),
    subscribe: vi.fn(async (callback) => { listener = callback; return unsubscribe; }),
  };
  return { source, pending, ready, unsubscribe, emit: (snapshot: StartupSnapshot) => listener(snapshot) };
}

// Pending startup waits for the event and releases its subscription after readiness.
// 未完成启动等待就绪事件，完成后释放订阅。
it("waits for runtime readiness and unsubscribes", async () => {
  const fixture = startupFixture();
  const settled = vi.fn();
  const waiting = waitForPluginStartup(fixture.source, new AbortController().signal).then(settled);
  await Promise.resolve(); await Promise.resolve();
  expect(settled).not.toHaveBeenCalled();
  fixture.emit(fixture.ready);
  await waiting;
  expect(settled).toHaveBeenCalledOnce();
  expect(fixture.unsubscribe).toHaveBeenCalledOnce();
});

// A ready event during the snapshot read wins over an older snapshot response.
// 快照读取期间的就绪事件优先于稍后返回的旧快照。
it("does not lose readiness between subscribe and snapshot", async () => {
  const fixture = startupFixture();
  fixture.source.read = vi.fn(async () => { fixture.emit(fixture.ready); return fixture.pending; });
  await waitForPluginStartup(fixture.source, new AbortController().signal);
  expect(fixture.unsubscribe).toHaveBeenCalledOnce();
});

// Startup failure is explicit and cleans up the listener.
// 启动失败明确抛出并清理监听器。
it("rejects failed startup and releases its subscription", async () => {
  const fixture = startupFixture();
  const waiting = waitForPluginStartup(fixture.source, new AbortController().signal);
  await Promise.resolve();
  fixture.emit({ ...fixture.pending, phase: "failed" });
  await expect(waiting).rejects.toThrow("Plugin runtime startup failed");
  expect(fixture.unsubscribe).toHaveBeenCalledOnce();
});

// Generation retirement cancels only the startup wait and its exact listener.
// 代次退役仅取消启动等待和对应监听器。
it("releases its subscription when the module generation is retired", async () => {
  const fixture = startupFixture();
  const cancellation = new AbortController();
  const waiting = waitForPluginStartup(fixture.source, cancellation.signal);
  await Promise.resolve(); await Promise.resolve();
  cancellation.abort();
  await expect(waiting).rejects.toMatchObject({ name: "AbortError" });
  expect(fixture.unsubscribe).toHaveBeenCalledOnce();
});

// Retirement before admission must not allocate a subscription.
// 入场前退役不能创建订阅。
it("does not subscribe after retirement", async () => {
  const fixture = startupFixture();
  const cancellation = new AbortController();
  cancellation.abort();
  await expect(waitForPluginStartup(fixture.source, cancellation.signal)).rejects.toMatchObject({ name: "AbortError" });
  expect(fixture.source.subscribe).not.toHaveBeenCalled();
});
