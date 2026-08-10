import { describe, expect, it, vi } from "vitest";

import {
  confirmedComposerPersistenceValue,
  enqueueComposerPersistence,
  flushLatestComposerPersistence,
} from "./composer-persistence-queue";

// createDeferred creates one externally controlled Promise for deterministic ordering tests.
// createDeferred 创建一个可从外部控制的 Promise，用于确定性顺序测试。
// Returns: the promise and its exact resolve/reject controls.
// 返回值：Promise 及其精确的 resolve/reject 控制函数。
function createDeferred<T>() {
  // resolve is assigned synchronously by the Promise constructor before this helper returns.
  // resolve 会在本辅助函数返回前由 Promise 构造器同步赋值。
  let resolve!: (value: T | PromiseLike<T>) => void;
  // reject is assigned synchronously by the Promise constructor before this helper returns.
  // reject 会在本辅助函数返回前由 Promise 构造器同步赋值。
  let reject!: (reason?: unknown) => void;
  // promise exposes the externally controlled completion boundary.
  // promise 暴露可从外部控制的完成边界。
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("enqueueComposerPersistence", () => {
  it("rolls the latest failed intent back to the last confirmed success", () => {
    // afterFirst stores the confirmed result of the first successful queued intent.
    // afterFirst 保存第一项成功排队意图的确认结果。
    const afterFirst = confirmedComposerPersistenceValue("initial", "first", true);
    // afterSecond must retain the first durable value when the newer intent fails.
    // afterSecond 必须在更新意图失败时保留第一项持久值。
    const afterSecond = confirmedComposerPersistenceValue(afterFirst, "second", false);
    expect(afterSecond).toBe("first");
  });

  it("runs overlapping writes in user-intent order", async () => {
    // firstCompletion keeps the first write pending while the second is enqueued.
    // firstCompletion 在第二项写入排队期间保持第一项写入未完成。
    const firstCompletion = createDeferred<boolean>();
    // calls records the exact order in which persistence callbacks begin.
    // calls 记录持久化回调开始执行的精确顺序。
    const calls: string[] = [];
    // first is the initial queued write.
    // first 是第一项排队写入。
    const first = enqueueComposerPersistence(Promise.resolve(), async () => {
      calls.push("first");
      return await firstCompletion.promise;
    });
    // second must remain behind the first queue tail.
    // second 必须保持在第一项队列尾部之后。
    const secondPersist = vi.fn(async () => {
      calls.push("second");
      return true;
    });
    // second is enqueued before the first operation settles.
    // second 在第一项操作完成前进入队列。
    const second = enqueueComposerPersistence(first.tail, secondPersist);

    await Promise.resolve();
    expect(calls).toEqual(["first"]);
    expect(secondPersist).not.toHaveBeenCalled();

    firstCompletion.resolve(true);
    await expect(first.result).resolves.toBe(true);
    await expect(second.result).resolves.toBe(true);
    expect(calls).toEqual(["first", "second"]);
  });

  it("continues the queue after one rejected write", async () => {
    // failed represents one transient persistence rejection.
    // failed 表示一次临时持久化拒绝。
    const failed = enqueueComposerPersistence(Promise.resolve(), async () => {
      throw new Error("write failed");
    });
    // recovered proves the non-rejecting tail admits the next user intent.
    // recovered 证明不会拒绝的尾部仍会放行下一项用户意图。
    const recovered = enqueueComposerPersistence(failed.tail, async () => true);

    await expect(failed.result).rejects.toThrow("write failed");
    await expect(recovered.result).resolves.toBe(true);
  });

  it("flushes a newer intent appended while an older result is pending", async () => {
    // firstCompletion keeps the observed former intent pending until the replacement exists.
    // firstCompletion 保持已观察到的旧意图待处理，直到替代意图已经存在。
    const firstCompletion = createDeferred<boolean>();
    // latestPending is the mutable queue head read by the submission barrier.
    // latestPending 是提交屏障读取的可变队列头。
    let latestPending: Promise<boolean> | null = firstCompletion.promise;
    // flushed starts before the newer intent is installed.
    // flushed 在更新意图安装前开始执行。
    const flushed = flushLatestComposerPersistence(() => latestPending);
    // secondCompletion becomes the authoritative latest intent during the first wait.
    // secondCompletion 在第一次等待期间成为权威的最新意图。
    const secondCompletion = createDeferred<boolean>();
    latestPending = secondCompletion.promise;
    firstCompletion.resolve(false);
    await Promise.resolve();
    secondCompletion.resolve(true);

    await expect(flushed).resolves.toBe(true);
  });

  it("returns the latest failed persistence result", async () => {
    // failed is the only observable intent and therefore owns the flush result.
    // failed 是唯一可观察意图，因此持有刷新结果。
    const failed = Promise.resolve(false);
    await expect(flushLatestComposerPersistence(() => failed)).resolves.toBe(false);
  });
});
