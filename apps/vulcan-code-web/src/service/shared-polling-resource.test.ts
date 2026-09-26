// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createSharedPollingResource } from "./shared-polling-resource";

const disposals: (() => void)[] = [];
beforeEach(() => { vi.useFakeTimers(); vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible"); });
afterEach(() => { for (const dispose of disposals.splice(0)) dispose(); vi.useRealTimers(); vi.restoreAllMocks(); });
const observer = () => ({ next: vi.fn(), error: vi.fn() });
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (failure: Error) => void;
  const promise = new Promise<T>((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
}
const flush = () => vi.advanceTimersByTimeAsync(0);

it("shares initial reads and timers, retaining reference identity on unchanged snapshots", async () => {
  const load = vi.fn(async () => ["same"]);
  const retired = vi.fn();
  const resource = createSharedPollingResource(load, 100, (a, b) => a.join() === b.join(), retired);
  const a = observer(), b = observer();
  disposals.push(resource.subscribe(a), resource.subscribe(b));
  await flush();
  expect(load).toHaveBeenCalledTimes(1);
  expect(a.next).toHaveBeenCalledTimes(1);
  expect(a.next.mock.calls[0][0]).toBe(b.next.mock.calls[0][0]);
  await vi.advanceTimersByTimeAsync(300);
  expect(load).toHaveBeenCalledTimes(4);
  expect(a.next).toHaveBeenCalledTimes(1);
  expect(b.next).toHaveBeenCalledTimes(1);
  disposals[0]();
  await vi.advanceTimersByTimeAsync(100);
  expect(load).toHaveBeenCalledTimes(5);
  expect(retired).not.toHaveBeenCalled();
  disposals[1]();
  await vi.advanceTimersByTimeAsync(1000);
  expect(load).toHaveBeenCalledTimes(5);
  expect(retired).toHaveBeenCalledTimes(1);
});

it("does not let old success hide a newer failure and republishes unchanged data on recovery", async () => {
  const old = deferred<number>();
  const load = vi.fn<() => Promise<number>>().mockResolvedValueOnce(1).mockImplementationOnce(() => old.promise)
    .mockRejectedValueOnce(new Error("newest failure")).mockResolvedValueOnce(1);
  const resource = createSharedPollingResource(load, 100, Object.is, vi.fn());
  const seen = observer(); disposals.push(resource.subscribe(seen)); await flush();
  const older = resource.refresh();
  const newer = resource.refresh().catch(() => {});
  await flush(); await newer;
  old.resolve(2); await older;
  expect(seen.next.mock.calls.map(([value]) => value)).toEqual([1]);
  expect(seen.error).toHaveBeenCalledOnce();
  await resource.refresh();
  expect(seen.next.mock.calls.map(([value]) => value)).toEqual([1, 1]);
});

it("ignores older failures after newer success without rejecting the periodic observer", async () => {
  const old = deferred<number>();
  const load = vi.fn<() => Promise<number>>().mockResolvedValueOnce(0).mockImplementationOnce(() => old.promise).mockResolvedValueOnce(2);
  const resource = createSharedPollingResource(load, 100, Object.is, vi.fn());
  const seen = observer(); disposals.push(resource.subscribe(seen)); await flush();
  const older = resource.refresh().catch(() => {});
  await resource.refresh(); old.reject(new Error("stale")); await older;
  expect(seen.next.mock.calls.map(([value]) => value)).toEqual([0, 2]);
  expect(seen.error).not.toHaveBeenCalled();
});

it("bounds overlapping manual refreshes and coalesces additional requests", async () => {
  const requests = [deferred<number>(), deferred<number>(), deferred<number>()];
  let index = 0;
  const load = vi.fn(() => requests[index++].promise);
  const resource = createSharedPollingResource(load, 100, Object.is, vi.fn());
  disposals.push(resource.subscribe(observer())); await flush();
  const second = resource.refresh();
  const queued = resource.refresh();
  expect(resource.refresh()).toBe(queued);
  await vi.advanceTimersByTimeAsync(1000);
  expect(load).toHaveBeenCalledTimes(2);
  requests[1].resolve(2); await second; await flush();
  expect(load).toHaveBeenCalledTimes(3);
  requests[2].resolve(3); expect(await queued).toBe(3);
  requests[0].resolve(1); await flush();
});

it("pauses hidden periodic work and refreshes once when visible", async () => {
  const load = vi.fn(async () => 1);
  const resource = createSharedPollingResource(load, 100, Object.is, vi.fn());
  disposals.push(resource.subscribe(observer())); await flush();
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(1000); expect(load).toHaveBeenCalledTimes(1);
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  document.dispatchEvent(new Event("visibilitychange")); await flush();
  expect(load).toHaveBeenCalledTimes(2);
});

it("retires a last-unsubscribed identity and never publishes late results", async () => {
  const pending = deferred<number>();
  const retired = vi.fn();
  const resource = createSharedPollingResource(() => pending.promise, 100, Object.is, retired);
  const seen = observer(); const dispose = resource.subscribe(seen); dispose(); dispose();
  pending.resolve(1); await flush();
  expect(seen.next).not.toHaveBeenCalled(); expect(retired).toHaveBeenCalledOnce();
  await expect(resource.refresh()).rejects.toThrow("ended");
  expect(() => resource.subscribe(observer())).toThrow("retired");
});
