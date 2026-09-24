import { afterEach, expect, it, vi } from "vitest";
import { createPluginQueryClient, invalidateAllPluginQueries, invalidatePluginQueries } from "./plugin-ui-query-runtime";

// Controlled completion proves navigation reuse without relying on wall-clock sleeps.
// 可控制的完成事件证明导航复用，不依赖实际时间睡眠。
function deferred() {
  let resolve!: (value: unknown) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<unknown>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// Host-owned identities stay fixed unless a test deliberately selects another scope.
// 宿主身份保持固定，除非用例明确选择其他作用域。
const context = { pluginId: "rtk", runtimeIdentity: "service:1", workspacePath: null, conversationId: null };
const policies = [
  { id: "read", effect: "read" as const, resultCacheMs: 5000 },
  { id: "uncached", effect: "read" as const, resultCacheMs: 0 },
  { id: "write", effect: "mutation" as const, resultCacheMs: 0 },
];
afterEach(() => { invalidateAllPluginQueries(); vi.useRealTimers(); });

// An invalidation emitted inside a write must refresh only after that write commits.
// 写入内部发出的失效必须在该写入提交之后才刷新。
it("waits for writes in the same incarnation without blocking a restarted service", async () => {
  const pending = deferred();
  const dispatch = vi.fn((action: string) => action === "write" ? pending.promise : Promise.resolve("fresh"));
  const client = createPluginQueryClient(context, policies, dispatch);
  const write = client.invoke("write", {});
  await Promise.resolve();
  invalidatePluginQueries(context.pluginId);
  const read = client.invoke("read", {});
  const restarted = createPluginQueryClient({ ...context, runtimeIdentity: "new-service:1" }, policies, dispatch);
  await expect(restarted.invoke("read", {})).resolves.toBe("fresh");
  expect(dispatch).toHaveBeenCalledTimes(2);
  pending.resolve("saved");
  await write;
  await expect(read).resolves.toBe("fresh");
  expect(dispatch).toHaveBeenCalledTimes(3);
});

// Filter cancellation waits for the last subscriber and withdraws the entry before replacement.
// 筛选取消等待最后一个订阅者，并在替换之前撤回条目。
it("withdraws superseded backend admission only after its last subscriber leaves", async () => {
  const pending = deferred();
  const dispatch = vi.fn((_action: string, _args: unknown, signal?: AbortSignal) => { expect(signal?.aborted).toBe(false); return pending.promise; });
  const first = createPluginQueryClient(context, policies, dispatch);
  const second = createPluginQueryClient(context, policies, dispatch);
  const filter = new AbortController();
  const one = first.invoke("read", {}, { signal: filter.signal }).catch(error => error.name);
  const two = second.invoke("read", {}).catch(error => error.name);
  await Promise.resolve();
  filter.abort();
  expect(dispatch.mock.calls[0][2]?.aborted).toBe(false);
  second.dispose();
  expect(dispatch.mock.calls[0][2]?.aborted).toBe(true);
  const next = first.invoke("read", {});
  await Promise.resolve();
  expect(dispatch).toHaveBeenCalledTimes(2);
  pending.resolve("replacement");
  expect(await one).toBe("AbortError");
  expect(await two).toBe("AbortError");
  await expect(next).resolves.toBe("replacement");
});

it("shares running work across ten mounts and retains an approved completion after unmount", async () => {
  const pending = deferred();
  const dispatch = vi.fn(() => pending.promise);
  const first = createPluginQueryClient(context, policies, dispatch);
  const abandoned = first.invoke("read", { b: 2, a: 1 }).catch(error => error.name);
  await Promise.resolve();
  first.dispose();
  for (let index = 0; index < 10; index++) {
    const client = createPluginQueryClient(context, policies, dispatch);
    const result = client.invoke("read", { a: 1, b: 2 }).catch(error => error.name);
    client.dispose();
    await expect(result).resolves.toBe("AbortError");
  }
  pending.resolve({ total: 42 });
  await expect(abandoned).resolves.toBe("AbortError");
  await Promise.resolve(); await Promise.resolve();
  const returning = createPluginQueryClient(context, policies, dispatch);
  await expect(returning.invoke("read", { a: 1, b: 2 })).resolves.toEqual({ total: 42 });
  expect(dispatch).toHaveBeenCalledTimes(1);
});

it("drops an unsubscribed queued read without dispatching it", async () => {
  const dispatch = vi.fn(async () => ({}));
  const client = createPluginQueryClient(context, policies, dispatch);
  const result = client.invoke("read", {}).catch(error => error.name);
  client.dispose();
  await expect(result).resolves.toBe("AbortError");
  expect(dispatch).not.toHaveBeenCalled();
});

it("joins repeated refreshes but refreshes a completed result", async () => {
  const pending = deferred();
  const dispatch = vi.fn().mockResolvedValueOnce({ total: 1 }).mockImplementation(() => pending.promise);
  const client = createPluginQueryClient(context, policies, dispatch);
  await client.invoke("read", {});
  const first = client.invoke("read", {}, { refresh: true });
  const second = client.invoke("read", {}, { refresh: true });
  pending.resolve({ total: 2 });
  expect(await first).toEqual(await second);
  expect(dispatch).toHaveBeenCalledTimes(2);
});

it("rejects stale completion after invalidation and allows a fresh retry", async () => {
  const pending = deferred();
  const dispatch = vi.fn().mockImplementationOnce(() => pending.promise).mockResolvedValue({ fresh: true });
  const client = createPluginQueryClient(context, policies, dispatch);
  const old = client.invoke("read", {});
  const rejected = expect(old).rejects.toThrow("stale");
  await Promise.resolve();
  invalidatePluginQueries(context.pluginId);
  pending.resolve({ stale: true });
  await rejected;
  await expect(client.invoke("read", {})).resolves.toEqual({ fresh: true });
});

it("keeps service, conversation, workspace and partial projections separate", async () => {
  const dispatch = vi.fn(async () => ({}));
  for (const owner of [context, { ...context, runtimeIdentity: "service:2" }, { ...context, conversationId: "conversation" }, { ...context, workspacePath: "D:/workspace" }]) {
    const client = createPluginQueryClient(owner, policies, dispatch);
    await client.invoke("read", { view: "overview", endAtMs: null });
    await client.invoke("read", { view: "daily-page", endAtMs: null });
    await client.invoke("read", { view: "overview", endAtMs: 123 });
  }
  expect(dispatch).toHaveBeenCalledTimes(12);
});

it("does not retain uncached reads or failures and never deduplicates mutations", async () => {
  const dispatch = vi.fn().mockRejectedValueOnce(new Error("failure")).mockResolvedValue({ ok: true });
  const client = createPluginQueryClient(context, policies, dispatch);
  await expect(client.invoke("read", {})).rejects.toThrow("failure");
  await client.invoke("read", {});
  await client.invoke("uncached", {});
  await client.invoke("uncached", {});
  await Promise.all([client.invoke("write", {}), client.invoke("write", {})]);
  await client.invoke("read", {});
  expect(dispatch).toHaveBeenCalledTimes(7);
});
