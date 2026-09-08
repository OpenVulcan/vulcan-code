import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceJobSummary } from "@/types";
import { EMPTY_VCE_OBSERVED_JOB, VceJobObserverStore, type VceJobObserverTransport } from "./vce-job-observer-store";
import {
  VCE_OBSERVER_READ_DEADLINE_MS,
  VCE_OBSERVER_READ_TIMEOUT_MESSAGE,
} from "./vce-observer-timeouts";

// Represent an explicitly controlled promise without timers or real service calls.
// 表示明确受控的承诺，不使用计时器或真实服务调用。
interface Deferred<T> {
  // promise is the value awaited by production code.
  // promise 为生产代码等待的值。
  promise: Promise<T>;
  // resolve completes with the supplied value; returns nothing.
  // resolve 以提供的值完成；无返回值。
  resolve(value: T): void;
  // reject completes with the supplied failure; returns nothing.
  // reject 以提供的错误完成；无返回值。
  reject(error: Error): void;
}

// Create a controlled promise of T; takes no arguments and returns its exact completion handles.
// 创建 T 类型受控承诺；无参数，返回其精确完成句柄。
function deferred<T>(): Deferred<T> {
  // resolve/reject are initialized synchronously by the Promise constructor.
  // resolve/reject 由承诺构造函数同步初始化。
  let resolve!: Deferred<T>["resolve"];
  let reject!: Deferred<T>["reject"];
  // promise captures completion without starting asynchronous external work.
  // promise 捕获完成句柄，不启动异步外部工作。
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

// abortAwareRead models fetch rejecting as soon as the caller aborts the transport.
// abortAwareRead 模拟调用方中止传输后 fetch 立即拒绝请求。
// Parameters: signal is the caller-owned abort signal observed by the fixture.
// 参数：signal 是测试替身监听的调用方中止信号。
// Returns: a promise rejected with AbortError on abort and otherwise left pending.
// 返回值：信号中止时以 AbortError 拒绝、否则保持 pending 的承诺。
function abortAwareRead<T>(signal: AbortSignal | undefined): Promise<T> {
  if (!signal) return Promise.reject(new Error("missing abort signal"));
  return new Promise<T>((_resolve, reject) => {
    const onAbort = () => {
      const error = new Error("request aborted");
      error.name = "AbortError";
      reject(error);
    };
    if (signal.aborted) onAbort();
    else signal.addEventListener("abort", onAbort, { once: true });
  });
}

// Build a complete running receipt for projectId; overrides select exact test state.
// 为 projectId 构建完整运行回执；overrides 选择精确测试状态。
// Returns an independent wire value without a synthetic host epoch field.
// 返回独立线协议值，不编造宿主代次字段。
function receipt(projectId = "project-a", overrides: Partial<VceJobSummary> = {}): VceJobSummary {
  return { projectId, jobId: "job-1", kind: "synchronize", state: "running", sequence: 2,
    createdAtUnixMs: 1000, updatedAtUnixMs: 2000, sourceCheckpointVersion: 1,
    resultCheckpointVersion: null, progress: null, followupRequested: false, error: null, ...overrides };
}

// Hold production store and typed transport substitutes separately for every test.
// 为每项测试分别保留生产仓库及有类型的传输替身。
let store: VceJobObserverStore;
// rpc exposes only the existing read/event/cancel contract, never indexing admission.
// rpc 仅暴露既有读取／事件／取消契约，绝不提供索引准入。
let rpc: { [K in keyof VceJobObserverTransport]: ReturnType<typeof vi.fn<VceJobObserverTransport[K]>> };
// releases owns test subscriptions so failed assertions cannot leak periodic observers.
// releases 持有测试订阅，使失败断言不能泄漏定时观察器。
let releases: Array<() => void>;

// Attach listener to projectId on the production store and retain its exact cleanup.
// 在生产仓库上为 projectId 挂接 listener，并保留其精确清理。
// Returns the idempotent subscription release for explicit lifetime tests.
// 返回幂等订阅释放函数，用于明确生命周期测试。
function observe(projectId: string | null, listener = vi.fn()): () => void {
  // release belongs only to this observation registration.
  // release 仅属于此观察注册。
  const release = store.subscribe(projectId, listener);
  releases.push(release);
  return release;
}

beforeEach(() => {
  vi.useFakeTimers();
  rpc = { load: vi.fn<VceJobObserverTransport["load"]>().mockResolvedValue(null),
    subscribe: vi.fn<VceJobObserverTransport["subscribe"]>().mockResolvedValue(() => undefined),
    cancel: vi.fn<VceJobObserverTransport["cancel"]>() };
  store = new VceJobObserverStore(rpc);
  releases = [];
});
afterEach(() => { for (const release of releases) release(); vi.useRealTimers(); });

describe("workbench-owned VCE job observation", () => {
  // Construction, snapshots and projectless views are strictly read-free.
  // 构造、快照及无项目视图严格不发起读取。
  it("does no IO until a real project is observed", async () => {
    expect(store.snapshot("project-a")).toBe(EMPTY_VCE_OBSERVED_JOB);
    expect(store.snapshot(null)).toBe(EMPTY_VCE_OBSERVED_JOB);
    observe(null);
    await vi.advanceTimersByTimeAsync(10000);
    expect(rpc.load).not.toHaveBeenCalled(); expect(rpc.subscribe).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  // Two views of one project share reads, cadence, events and immutable snapshot identity.
  // 同项目两个视图共享读取、节拍、事件及不可变快照身份。
  it("shares one read chain and releases it only after the last view closes", async () => {
    const stop = vi.fn(); rpc.subscribe.mockResolvedValue(stop);
    const first = vi.fn(); const second = vi.fn();
    const closeFirst = observe("project-a", first); const closeSecond = observe("project-a", second);
    await vi.advanceTimersByTimeAsync(0);
    expect(rpc.load).toHaveBeenCalledTimes(1); expect(rpc.subscribe).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(1);
    const job = receipt(); rpc.subscribe.mock.calls[0][0](job);
    expect(store.snapshot("project-a").job).toBe(job);
    expect(first).toHaveBeenCalledTimes(1); expect(second).toHaveBeenCalledTimes(1);
    const shared = store.snapshot("project-a"); rpc.subscribe.mock.calls[0][0]({ ...job });
    expect(store.snapshot("project-a")).toBe(shared);
    closeFirst(); closeFirst(); await vi.advanceTimersByTimeAsync(5000);
    expect(rpc.load).toHaveBeenCalledTimes(2); expect(stop).not.toHaveBeenCalled();
    closeSecond(); expect(stop).toHaveBeenCalledTimes(1); expect(vi.getTimerCount()).toBe(0);
    expect(store.snapshot("project-a")).toBe(EMPTY_VCE_OBSERVED_JOB); expect(rpc.cancel).not.toHaveBeenCalled();
  });

  // A shared event stream does not make unrelated project listeners rerender.
  // 共享事件流不会导致无关项目监听者重新渲染。
  it("isolates distinct project projections on one stream", async () => {
    const first = vi.fn(); const second = vi.fn();
    observe("project-a", first); observe("project-b", second);
    await vi.advanceTimersByTimeAsync(0);
    expect(rpc.subscribe).toHaveBeenCalledTimes(1); expect(rpc.load).toHaveBeenCalledTimes(2);
    rpc.subscribe.mock.calls[0][0](receipt()); rpc.subscribe.mock.calls[0][0](receipt("unmounted"));
    expect(first).toHaveBeenCalledTimes(1); expect(second).not.toHaveBeenCalled();
    expect(store.snapshot("project-b")).toBe(EMPTY_VCE_OBSERVED_JOB);
    expect(store.snapshot("unmounted")).toBe(EMPTY_VCE_OBSERVED_JOB);
  });

  // Slow requests do not multiply at every polling tick or overwrite events that overtook them.
  // 慢请求不在每次轮询时倍增，也不覆盖先于其到达的事件。
  it("coalesces slow polling and rejects stale or empty read results", async () => {
    const read = deferred<VceJobSummary | null>(); rpc.load.mockReturnValueOnce(read.promise);
    observe("project-a"); await vi.advanceTimersByTimeAsync(15000);
    expect(rpc.load).toHaveBeenCalledTimes(1);
    const newer = receipt("project-a", { sequence: 8 }); rpc.subscribe.mock.calls[0][0](newer);
    read.resolve(receipt()); await vi.advanceTimersByTimeAsync(0);
    expect(store.snapshot("project-a").job).toBe(newer);
    expect(rpc.load).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(5000);
    expect(rpc.load).toHaveBeenCalledTimes(2); expect(store.snapshot("project-a").job).toBe(newer);
  });

  // Read ownership failures remain explicit and cannot inject another project's job.
  // 读取归属错误保持明确，不能注入另一项目作业。
  it("reports mismatched reads without erasing the last valid receipt", async () => {
    rpc.load.mockResolvedValue(receipt("project-b")); observe("project-a");
    await vi.advanceTimersByTimeAsync(0);
    expect(store.snapshot("project-a").error).toContain("其他项目");
    rpc.subscribe.mock.calls[0][0](receipt());
    expect(store.snapshot("project-a").job?.projectId).toBe("project-a");
    expect(store.snapshot("project-a").error).toContain("其他项目");
    rpc.load.mockResolvedValue(null); await vi.advanceTimersByTimeAsync(5000);
    expect(store.snapshot("project-a").error).toBeNull();
    expect(store.snapshot("project-a").job?.projectId).toBe("project-a");
    expect(() => store.receive("project-a", receipt("project-b"))).toThrow("身份不匹配");
  });

  // Same-ID remounts cannot consume a retired read or event callback; late handles are released.
  // 相同编号重新挂载不能接收已退役读取或事件回调；迟到句柄会被释放。
  it("fences retired registration and read lifetimes across same-project remounts", async () => {
    const read = deferred<VceJobSummary | null>(); const registration = deferred<() => void>(); const retiredStop = vi.fn();
    rpc.load.mockReturnValueOnce(read.promise); rpc.subscribe.mockReturnValueOnce(registration.promise);
    const close = observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    const oldCallback = rpc.subscribe.mock.calls[0][0]; close();
    observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    registration.resolve(retiredStop); read.resolve(receipt("project-a", { sequence: 90 }));
    oldCallback(receipt("project-a", { sequence: 100 })); await vi.advanceTimersByTimeAsync(0);
    expect(retiredStop).toHaveBeenCalledTimes(1); expect(store.snapshot("project-a").job).toBeNull();
    rpc.subscribe.mock.calls[1][0](receipt()); expect(store.snapshot("project-a").job?.sequence).toBe(2);
  });

  // Retiring before the registration microtask runs should not create a transient external stream.
  // 在注册微任务运行之前退役，不应创建临时外部事件流。
  it("does not register an event stream for an already retired observer", async () => {
    observe("project-a")(); await vi.advanceTimersByTimeAsync(0);
    expect(rpc.subscribe).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
  });

  // Retiring a read aborts only the observer transport; deadlines remain retryable and never issue cancellation.
  // 观察退役只中止观察传输；期限错误可重试，且绝不发出作业取消。
  it("aborts retired reads, reports deadline reads, and never cancels the host job", async () => {
    let retiredSignal: AbortSignal | undefined;
    rpc.load.mockImplementationOnce((_projectId, options) => {
      retiredSignal = options?.signal;
      return abortAwareRead(options?.signal);
    });
    const closeRetired = observe("project-a");
    await vi.advanceTimersByTimeAsync(0);
    expect(retiredSignal?.aborted).toBe(false);
    closeRetired();
    expect(retiredSignal?.aborted).toBe(true);
    expect(rpc.cancel).not.toHaveBeenCalled();

    let deadlineSignal: AbortSignal | undefined;
    rpc.load.mockImplementationOnce((_projectId, options) => {
      deadlineSignal = options?.signal;
      return abortAwareRead(options?.signal);
    });
    const reopened = observe("project-a");
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(VCE_OBSERVER_READ_DEADLINE_MS);
    expect(deadlineSignal?.aborted).toBe(true);
    await vi.advanceTimersByTimeAsync(0);
    expect(store.snapshot("project-a").error).toBe(VCE_OBSERVER_READ_TIMEOUT_MESSAGE);

    rpc.load.mockResolvedValueOnce(null);
    await vi.advanceTimersByTimeAsync(5000);
    expect(store.snapshot("project-a").error).toBeNull();
    expect(rpc.cancel).not.toHaveBeenCalled();
    reopened();
    expect(vi.getTimerCount()).toBe(0);
  });

  // Stream failures cannot be hidden by healthy polling and are retried at a bounded cadence.
  // 正常轮询不能掩盖流失败，流失败按有界节拍重试。
  it("retains connection failures until the shared subscription recovers", async () => {
    rpc.subscribe.mockRejectedValueOnce(new Error("event stream unavailable"));
    observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    expect(store.snapshot("project-a").error).toBe("event stream unavailable");
    const read = deferred<VceJobSummary | null>(); rpc.load.mockReturnValueOnce(read.promise);
    observe("project-b"); expect(store.snapshot("project-b").error).toBe("event stream unavailable");
    await vi.advanceTimersByTimeAsync(0);
    expect(rpc.subscribe).toHaveBeenCalledTimes(2);
    expect(store.snapshot("project-a").error).toBeNull(); expect(store.snapshot("project-b").error).toBeNull();
    read.resolve(null); await vi.advanceTimersByTimeAsync(5000);
    expect(rpc.subscribe).toHaveBeenCalledTimes(2);
  });

  // A rejected subscription retry cannot form an immediate promise loop.
  // 失败的订阅重试不能形成立即执行的承诺循环。
  it("retries a failed event stream once per polling interval", async () => {
    rpc.subscribe.mockRejectedValue(new Error("offline")); observe("project-a");
    await vi.advanceTimersByTimeAsync(14999);
    expect(rpc.subscribe).toHaveBeenCalledTimes(3); expect(rpc.load).toHaveBeenCalledTimes(3);
    expect(store.snapshot("project-a").error).toBe("offline");
  });

  // Exact-job cancellation deduplicates commands across views without blocking a replacement job.
  // 精确作业取消跨视图去重命令，不阻塞替代作业。
  it("shares cancellation while protecting a newer job from delayed command results", async () => {
    rpc.load.mockResolvedValue(receipt()); observe("project-a"); observe("project-a");
    await vi.advanceTimersByTimeAsync(0);
    const oldCancel = deferred<VceJobSummary>(); const newCancel = deferred<VceJobSummary>();
    rpc.cancel.mockReturnValueOnce(oldCancel.promise).mockReturnValueOnce(newCancel.promise);
    const first = store.cancel("project-a", "job-1"); const same = store.cancel("project-a", "job-1");
    expect(first).toBe(same); expect(store.snapshot("project-a").cancelPending).toBe(true);
    await vi.advanceTimersByTimeAsync(0); expect(rpc.cancel).toHaveBeenCalledTimes(1);
    const newer = receipt("project-a", { jobId: "job-2", createdAtUnixMs: 4000 }); store.receive("project-a", newer);
    expect(store.snapshot("project-a").cancelPending).toBe(false);
    await store.cancel("project-a", "job-1"); expect(rpc.cancel).toHaveBeenCalledTimes(1);
    const second = store.cancel("project-a", "job-2"); await vi.advanceTimersByTimeAsync(0);
    expect(rpc.cancel).toHaveBeenNthCalledWith(2, "project-a", "job-2");
    oldCancel.resolve(receipt("project-a", { state: "cancelled", sequence: 9 })); await first;
    expect(store.snapshot("project-a").job).toBe(newer); expect(store.snapshot("project-a").cancelPending).toBe(true);
    newCancel.resolve({ ...newer, sequence: 3, state: "cancelled" }); await second;
    expect(store.snapshot("project-a").job?.state).toBe("cancelled"); expect(store.snapshot("project-a").cancelPending).toBe(false);
  });

  // Cancellation errors remain attached to the requested active job and clear on a later job.
  // 取消错误保持绑定到被请求的活动作业，在后续作业出现时清除。
  it("rejects mismatched cancel receipts and allows an explicit retry", async () => {
    rpc.load.mockResolvedValue(receipt()); observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    rpc.cancel.mockResolvedValueOnce(receipt("project-b")); await store.cancel("project-a", "job-1");
    expect(store.snapshot("project-a").error).toContain("不匹配"); expect(store.snapshot("project-a").cancelPending).toBe(false);
    await vi.advanceTimersByTimeAsync(5000); expect(store.snapshot("project-a").error).toContain("不匹配");
    rpc.cancel.mockResolvedValueOnce(receipt("project-a", { state: "cancelled", sequence: 3 }));
    await store.cancel("project-a", "job-1"); expect(store.snapshot("project-a").error).toBeNull();
    expect(store.snapshot("project-a").job?.state).toBe("cancelled");
  });

  // Closing a view does not issue cancellation and late command failures cannot poison a remount.
  // 关闭视图不发出取消，迟到的命令错误不能污染重新挂载。
  it("discards delayed cancellation failure after the last observer leaves", async () => {
    rpc.load.mockResolvedValue(receipt()); const close = observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    const command = deferred<VceJobSummary>(); rpc.cancel.mockReturnValueOnce(command.promise);
    const cancel = store.cancel("project-a", "job-1"); await vi.advanceTimersByTimeAsync(0);
    close(); observe("project-a"); await vi.advanceTimersByTimeAsync(0);
    command.reject(new Error("retired command")); await cancel;
    expect(store.snapshot("project-a").error).toBeNull(); expect(store.snapshot("project-a").cancelPending).toBe(false);
    expect(rpc.cancel).toHaveBeenCalledTimes(1);
  });
});
