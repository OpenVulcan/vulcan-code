// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import type { CoreBrowserStreamCallbacks } from "./core-browser-service";
import { maintainCoreBrowserStream } from "./browser-stream-reconnect";

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });
const callbacks = (): CoreBrowserStreamCallbacks => ({ onFrame: vi.fn(), onState: vi.fn(), onError: vi.fn() });

it("backs off transient failures with a bounded jittered delay and stops all retries on disposal", async () => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  const open = vi.fn().mockRejectedValue(new Error("offline"));
  const stop = maintainCoreBrowserStream(open, callbacks());
  await vi.advanceTimersByTimeAsync(0);
  expect(open).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(749); expect(open).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(1); expect(open).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1_499); expect(open).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1); expect(open).toHaveBeenCalledTimes(3);
  for (let index = 0; index < 10; index += 1) await vi.advanceTimersByTimeAsync(30_000);
  expect(open.mock.calls.length).toBeLessThan(25);
  stop(); const issued = open.mock.calls.length;
  await vi.advanceTimersByTimeAsync(120_000);
  expect(open).toHaveBeenCalledTimes(issued);
  expect(vi.getTimerCount()).toBe(0);
});

it("ignores late events from replaced sockets and coalesces duplicate disconnections", async () => {
  vi.useFakeTimers(); vi.spyOn(Math, "random").mockReturnValue(0);
  const handlers: CoreBrowserStreamCallbacks[] = [];
  const closes: ReturnType<typeof vi.fn>[] = [];
  const open = vi.fn(async (next: CoreBrowserStreamCallbacks) => {
    handlers.push(next); const close = vi.fn(); closes.push(close); return close;
  });
  const output = callbacks(); const stop = maintainCoreBrowserStream(open, output);
  await vi.advanceTimersByTimeAsync(0);
  const first = handlers[0]; first.onState("disconnected"); first.onState("disconnected");
  expect(vi.getTimerCount()).toBe(1);
  await vi.advanceTimersByTimeAsync(750);
  expect(open).toHaveBeenCalledTimes(2);
  first.onError("stale error"); first.onState("disconnected");
  expect(output.onError).not.toHaveBeenCalled();
  expect(closes[0]).toHaveBeenCalledTimes(1);
  stop(); expect(closes[1]).toHaveBeenCalledTimes(1);
});

it("does not retry an explicit policy refusal and closes a late-opened socket after disposal", async () => {
  vi.useFakeTimers();
  let deliver: ((close: () => void) => void) | undefined;
  let handler: CoreBrowserStreamCallbacks | undefined;
  const open = vi.fn((next: CoreBrowserStreamCallbacks) => {
    handler = next; return new Promise<() => void>(resolve => { deliver = resolve; });
  });
  const stop = maintainCoreBrowserStream(open, callbacks());
  await vi.advanceTimersByTimeAsync(0);
  handler!.onFailure!({ code: "browser_stream_refused", retryable: false });
  handler!.onState("disconnected");
  const close = vi.fn(); deliver!(close);
  await vi.advanceTimersByTimeAsync(120_000);
  expect(close).toHaveBeenCalledTimes(1); expect(open).toHaveBeenCalledTimes(1);
  stop(); expect(vi.getTimerCount()).toBe(0);
});
