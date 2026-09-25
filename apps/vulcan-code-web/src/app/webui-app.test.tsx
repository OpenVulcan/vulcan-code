// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WebuiApp } from "./webui-app";
import type { StartupSnapshot } from "@/types";

// snapshot and retryable replace only startup service boundaries; the real boot component remains mounted.
// snapshot 与 retryable 仅替换启动服务边界；实际启动组件保持挂载。
const { snapshot, retryable } = vi.hoisted(() => ({ snapshot: vi.fn<() => Promise<StartupSnapshot>>(), retryable: vi.fn<() => boolean>() }));
vi.mock("@/service/workbench-snapshot-service", () => ({ loadStartupSnapshot: snapshot }));
vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true, serviceBootstrapErrorIsRetryable: retryable }));
vi.mock("./workbench-app", () => ({ WorkbenchApp: () => <p>Mounted workbench</p> }));
// waiting is an explicit non-ready snapshot, not an invented completion percentage.
// waiting 为明确的未就绪快照，不虚构完成百分比。
const waiting: StartupSnapshot = { ready: false, phase: "startup", uiLocale: "en-US", steps: [], recentLogs: [] };
// host and root isolate each boot lifecycle from the actual application and its settings.
// host 与 root 将每个启动生命周期与实际应用及其设置隔离。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.useFakeTimers(); snapshot.mockReset(); retryable.mockReset();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

it("mounts the workbench only after readiness and then stops startup polling", async () => {
  snapshot.mockResolvedValueOnce(waiting).mockResolvedValueOnce({ ...waiting, ready: true, phase: "ready" });
  await act(async () => root.render(<WebuiApp />));
  expect(host.textContent).not.toContain("Mounted workbench");
  expect(snapshot).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTimeAsync(500));
  expect(host.textContent).toBe("Mounted workbench");
  expect(vi.getTimerCount()).toBe(0);
  await act(async () => vi.advanceTimersByTimeAsync(5000));
  expect(snapshot).toHaveBeenCalledTimes(2);
});

it("keeps only one in-flight probe and ignores its result after unmount", async () => {
  // resolve completes the exact outstanding request after its owning screen has gone away.
  // resolve 在所属界面离开后完成精确的在途请求。
  let resolve!: (value: StartupSnapshot) => void;
  snapshot.mockReturnValue(new Promise<StartupSnapshot>((complete) => { resolve = complete; }));
  await act(async () => root.render(<WebuiApp />));
  await act(async () => vi.advanceTimersByTimeAsync(5000));
  expect(snapshot).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null));
  await act(async () => resolve({ ...waiting, ready: true }));
  expect(host.textContent).toBe("");
  expect(vi.getTimerCount()).toBe(0);
});

it("shows terminal request errors and does not retry or reload automatically", async () => {
  snapshot.mockRejectedValue(new Error("Synthetic permanent startup error"));
  retryable.mockReturnValue(false);
  await act(async () => root.render(<WebuiApp />));
  expect(host.textContent).toContain("Synthetic permanent startup error");
  expect(host.querySelector("button")).not.toBeNull();
  await act(async () => vi.advanceTimersByTimeAsync(5000));
  expect(snapshot).toHaveBeenCalledTimes(1);
  expect(vi.getTimerCount()).toBe(0);
});

it("bounds transient retry delay and cancels scheduled probes on unmount", async () => {
  snapshot.mockRejectedValue(new Error("Synthetic transient transport error")); retryable.mockReturnValue(true);
  await act(async () => root.render(<WebuiApp />));
  expect(snapshot).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTimeAsync(249));
  expect(snapshot).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTimeAsync(1));
  expect(snapshot).toHaveBeenCalledTimes(2);
  await act(async () => vi.advanceTimersByTimeAsync(500 + 1000 + 1500));
  expect(snapshot).toHaveBeenCalledTimes(5);
  expect(vi.getTimerCount()).toBe(1);
  await act(async () => root.render(null));
  expect(vi.getTimerCount()).toBe(0);
});

it("preserves the failed-step diagnostic while observing a later service recovery", async () => {
  snapshot.mockResolvedValueOnce({ ...waiting, phase: "failed", recentLogs: ["Less specific log"],
    steps: [{ id: "core", label: "Core", status: "error", message: "Exact startup failure" }] })
    .mockResolvedValueOnce({ ...waiting, ready: true, phase: "ready" });
  await act(async () => root.render(<WebuiApp />));
  expect(host.textContent).toContain("Exact startup failure");
  expect(host.textContent).not.toContain("Less specific log");
  await act(async () => vi.advanceTimersByTimeAsync(500));
  expect(host.textContent).toBe("Mounted workbench");
  expect(vi.getTimerCount()).toBe(0);
});
