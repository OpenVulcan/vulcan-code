import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceProjectActivityResponse, VceProjectChangedEvent } from "@/types";
import { VceActivityObserverStore } from "./vce-activity-observer-store";
import { createVceActivityFixture } from "./vce-activity-test-fixture";

// Create a manually settled promise of T; returns the promise plus exact resolve/reject controls.
// 创建手动完成的 T 类型承诺；返回承诺及精确完成、拒绝控制。
function deferred<T>() {
  // resolve/reject are assigned synchronously by the Promise constructor.
  // resolve/reject 由 Promise 构造函数同步赋值。
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  // promise holds the test-controlled transport boundary.
  // promise 持有测试控制的传输边界。
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// Build read-only transport mocks and a real store; returns observed calls and its event delivery path.
// 构建只读传输替身及真实仓库；返回可观察调用及其事件投递路径。
function fixture() {
  // listener is the current installed host invalidation callback.
  // listener 为当前安装的宿主失效回调。
  let listener!: (event: VceProjectChangedEvent) => void;
  // load produces full activity for each exact requested owner.
  // load 为每个精确请求的所有者生成完整活动。
  const load = vi.fn(async (projectId: string) => createVceActivityFixture(projectId));
  // stop tracks release of the one workbench event subscription.
  // stop 跟踪唯一工作台事件订阅的释放。
  const stop = vi.fn();
  // subscribe exposes registration failure and acknowledgement without a real service connection.
  // subscribe 暴露注册失败和确认，不连接真实服务。
  const subscribe = vi.fn(async (callback: (event: VceProjectChangedEvent) => void): Promise<() => void> => { listener = callback; return stop; });
  // store executes production observation logic with only the transport replaced.
  // store 仅替换传输，执行生产观察逻辑。
  const store = new VceActivityObserverStore({ load, subscribe });
  return { store, load, subscribe, stop, emit: (projectId = "project-a") => listener({ projectId, settingsRevision: 4, lifecycle: "ready" }) };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe("workbench activity observation", () => {
  // Independent workbenches converge after dropped events and a slow read without sharing UI ownership.
  // 独立工作台在丢事件和慢读取后收敛，不共享界面归属。
  // No inputs or output; only the read-only service boundary is replaced with controlled replies.
  // 无输入或输出；仅以受控响应替换只读服务边界。
  it("converges two independent workbenches after lost events, slow reads and reopening", async () => {
    // The host revision is shared authority, not a client-to-client synchronization shortcut.
    // 宿主修订为共享权威，而不是客户端之间同步的捷径。
    let revision = 2;
    // Each fixture owns a distinct production store and a separate subscription lifecycle.
    // 每个夹具持有独立生产仓库和独立订阅生命周期。
    const first = fixture();
    // The second workbench cannot inherit the first workbench's timers, errors or cleanup.
    // 第二个工作台不能继承第一个工作台的计时器、错误或清理。
    const second = fixture();
    first.load.mockImplementation(async (projectId) => createVceActivityFixture(projectId, revision));
    second.load.mockImplementation(async (projectId) => createVceActivityFixture(projectId, revision));
    // Both exact subscriptions observe the same stable project independently.
    // 两个精确订阅独立观察同一个稳定项目。
    const closeFirst = first.store.subscribe("project-a", vi.fn());
    // Closing either subscription must not retire the other window's observation.
    // 关闭任一订阅均不能使另一窗口的观察退役。
    const closeSecond = second.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(first.subscribe).toHaveBeenCalledTimes(1);
    expect(second.subscribe).toHaveBeenCalledTimes(1);
    // This response captures an older host revision and remains slower than two polling intervals.
    // 此响应捕获较旧宿主修订，并慢于两个轮询周期。
    const delayed = deferred<VceProjectActivityResponse>();
    first.load.mockClear().mockImplementationOnce(() => delayed.promise);
    // Retain the exact pending read while only the other workbench receives an invalidation.
    // 保留精确在途读取，同时仅让另一工作台收到失效事件。
    const reading = first.store.refresh("project-a");
    await vi.advanceTimersByTimeAsync(0);
    revision = 3;
    second.emit();
    await vi.advanceTimersByTimeAsync(10000);
    expect(first.load).toHaveBeenCalledTimes(1);
    expect(second.store.snapshot("project-a").activity?.settings.revision).toBe(3);
    delayed.resolve(createVceActivityFixture("project-a", 2));
    await reading;
    // Recovery polling must eventually discover the missed revision without any event replay.
    // 恢复轮询必须最终发现丢失的修订，无需重放事件。
    await vi.advanceTimersByTimeAsync(5000);
    expect(first.store.snapshot("project-a").activity?.settings.revision).toBe(3);
    expect(second.store.snapshot("project-a").activity?.settings.revision).toBe(3);
    closeFirst();
    revision = 4;
    second.emit();
    await vi.advanceTimersByTimeAsync(0);
    expect(first.stop).toHaveBeenCalledTimes(1);
    expect(second.stop).not.toHaveBeenCalled();
    expect(second.store.snapshot("project-a").activity?.settings.revision).toBe(4);
    // Reopening performs a fresh authoritative read rather than restoring the retired client snapshot.
    // 重开执行新的权威读取，而不是恢复退役客户端快照。
    const closeReopened = first.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(first.store.snapshot("project-a").activity?.settings.revision).toBe(4);
    expect(first.subscribe).toHaveBeenCalledTimes(2);
    expect(first.store.snapshot("project-a").error).toBeNull();
    expect(second.store.snapshot("project-a").error).toBeNull();
    closeReopened();
    closeSecond();
    expect(vi.getTimerCount()).toBe(0);
  });

  // Multiple surfaces of one project share reads and subscription; unrelated projects remain separate.
  // 同项目多个界面共享读取和订阅；无关项目保持独立。
  it("shares one stream and one polling chain per observed project", async () => {
    const f = fixture();
    const a = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    const initialReads = f.load.mock.calls.length;
    const otherA = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(f.load).toHaveBeenCalledTimes(initialReads);
    const b = f.store.subscribe("project-b", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(f.subscribe).toHaveBeenCalledTimes(1);
    f.load.mockClear();
    await vi.advanceTimersByTimeAsync(5000);
    expect(f.load.mock.calls).toEqual([["project-a"], ["project-b"]]);
    a(); expect(f.stop).not.toHaveBeenCalled();
    otherA(); expect(f.stop).not.toHaveBeenCalled();
    b(); expect(f.stop).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  // A hundred invalidations during one request require one follow-up and never publish the stale result.
  // 同一请求期间的一百次失效仅要求一次后续读取，绝不发布陈旧结果。
  it.each(["success", "failure"] as const)("coalesces invalidations across a slow read ending in %s", async (outcome) => {
    const f = fixture();
    const stop = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    const delayed = deferred<VceProjectActivityResponse>();
    f.load.mockClear().mockImplementationOnce(() => delayed.promise).mockResolvedValue(createVceActivityFixture("project-a", 6));
    const reading = f.store.refresh("project-a");
    await vi.advanceTimersByTimeAsync(0);
    for (let index = 0; index < 100; index += 1) f.emit();
    if (outcome === "failure") delayed.reject(new Error("superseded read failed"));
    else delayed.resolve(createVceActivityFixture("project-a", 5));
    await reading;
    expect(f.load).toHaveBeenCalledTimes(2);
    expect(f.store.snapshot("project-a").activity?.settings.revision).toBe(6);
    expect(f.store.snapshot("project-a").error).toBeNull();
    stop();
  });

  // Recovery polling cannot continually invalidate a response slower than the polling period.
  // 恢复轮询不能持续使慢于轮询周期的响应失效。
  it("does not starve a slow read with polling ticks", async () => {
    const f = fixture();
    const stop = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    const delayed = deferred<VceProjectActivityResponse>();
    f.load.mockClear().mockImplementationOnce(() => delayed.promise);
    const reading = f.store.refresh("project-a");
    await vi.advanceTimersByTimeAsync(50000);
    expect(f.load).toHaveBeenCalledTimes(1);
    delayed.resolve(createVceActivityFixture("project-a", 7));
    await reading;
    expect(f.store.snapshot("project-a").activity?.settings.revision).toBe(7);
    expect(f.load).toHaveBeenCalledTimes(1);
    stop();
  });

  // Reopening the same ID must not grant an old request authority over the new observation.
  // 重开相同编号不能授予旧请求覆盖新观察的权限。
  it("drops a retired lifetime even after returning to the same project", async () => {
    const f = fixture();
    const delayed = deferred<VceProjectActivityResponse>();
    f.load.mockImplementationOnce(() => delayed.promise);
    const old = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    old();
    f.load.mockResolvedValue(createVceActivityFixture("project-a", 8));
    const current = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    delayed.resolve(createVceActivityFixture("project-a", 3));
    await vi.advanceTimersByTimeAsync(0);
    expect(f.store.snapshot("project-a").activity?.settings.revision).toBe(8);
    current();
  });

  // Invalid cross-project reads expose an error while preserving the prior verified owner.
  // 非法跨项目读取暴露错误，同时保留先前验证的所有者。
  it("rejects mismatched project identity without mutating saved policy", async () => {
    const f = fixture();
    const stop = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    f.load.mockResolvedValue(createVceActivityFixture("project-b", 9));
    await f.store.refresh("project-a");
    expect(f.store.snapshot("project-a").error).toContain("其他项目");
    expect(f.store.snapshot("project-a").activity?.settings.enabled).toBe(true);
    expect(f.store.snapshot("project-a").activity?.status.projectId).toBe("project-a");
    stop();
  });

  // Failed registration is retried at a bounded poll and late successful registration is disposed.
  // 注册失败在有界轮询时重试；迟到的成功注册被释放。
  it("keeps stream errors separate and retries registration without a busy loop", async () => {
    const f = fixture();
    f.subscribe.mockRejectedValueOnce(new Error("stream unavailable"));
    const stop = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    expect(f.store.snapshot("project-a").activity).not.toBeNull();
    expect(f.store.snapshot("project-a").error).toBe("stream unavailable");
    await f.store.refresh("project-a");
    expect(f.subscribe).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(5000);
    expect(f.subscribe).toHaveBeenCalledTimes(2);
    expect(f.store.snapshot("project-a").error).toBeNull();
    stop();
    const registration = deferred<() => void>();
    f.subscribe.mockImplementationOnce(() => registration.promise);
    const release = f.store.subscribe("project-a", vi.fn());
    await vi.advanceTimersByTimeAsync(0);
    release();
    const lateStop = vi.fn();
    registration.resolve(lateStop);
    await vi.advanceTimersByTimeAsync(0);
    expect(lateStop).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  // Unknown-project events and projectless views cannot inspect historical projects.
  // 未知项目事件和无项目视图不能检查历史项目。
  it("performs no IO for projectless or unobserved owners", async () => {
    const f = fixture();
    const stop = f.store.subscribe(null, vi.fn());
    await f.store.refresh("unobserved");
    await vi.advanceTimersByTimeAsync(10000);
    expect(f.load).not.toHaveBeenCalled();
    expect(f.subscribe).not.toHaveBeenCalled();
    stop();
  });
});
