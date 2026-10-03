// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StartupDownloadProgress } from "./startup-download-progress";
import { StartupOverlay } from "./startup-screen";
import { BootErrorScreen, BootLoadingScreen, STARTUP_SURFACE_FADE_MS } from "@/app/boot-startup-screen";
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("./startup-logo-mark", () => ({ StartupLogoMark: () => null }));
describe("startup controls", () => {
  let root: Root;
  let host: HTMLDivElement;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
  // Long diagnostics remain available without hiding the single explicit recovery action.
  // 长诊断保持可查，不隐藏唯一的显式恢复操作。
  it("keeps startup failure details collapsed and reloads only on user action", async () => {
    // reload records the user's recovery intent, independent of error length.
    // reload 记录用户恢复意图，与错误长度独立。
    const reload = vi.fn();
    await act(async () => root.render(<BootErrorScreen error={"SyntheticFailure".repeat(100)} uiLocale="zh-CN" onReload={reload} />));
    expect(host.querySelector('details')?.open).toBe(false);
    expect(host.querySelector('summary')?.textContent).toBe("查看详情");
    expect(host.querySelector('p')?.textContent).toBe("SyntheticFailure".repeat(100));
    expect(reload).not.toHaveBeenCalled();
    await act(async () => host.querySelector<HTMLButtonElement>('button')!.click());
    expect(reload).toHaveBeenCalledOnce();
    await act(async () => root.render(<BootErrorScreen error="Failure" uiLocale="en-US" />));
    expect(host.querySelector('summary')?.textContent).toBe("View details");
    expect(host.querySelector('button')).toBeNull();
  });
  it("uses the same unknown and measured progress contract in the actual boot entry", async () => {
    await act(async () => root.render(<BootLoadingScreen />));
    expect(host.querySelector('[role="progressbar"]')?.hasAttribute("aria-valuenow")).toBe(false);
    expect(host.textContent).not.toContain("8%");
    await act(async () => root.render(<BootLoadingScreen startup={{ ready: false, phase: "startup", recentLogs: [], uiLocale: "en-US", steps: [
      { id: "done", label: "Done", status: "ready" }, { id: "active", label: "Active", status: "running", progressPercent: 37 },
    ] }} />));
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("37");
    expect(host.textContent).toContain("Completed 1/2 steps");
  });
  it("exposes a failed step ahead of pending steps and releases busy announcements", async () => {
    await act(async () => root.render(<StartupOverlay startup={{ ready: false, phase: "startup", recentLogs: [], steps: [
      { id: "pending", label: "Pending", status: "pending", message: "not started" },
      { id: "failed", label: "Failed", status: "error", message: "synthetic failure" },
    ] }} />));
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("synthetic failure");
    expect(host.querySelector(".startup-progress-panel")?.getAttribute("aria-busy")).toBe("false");
  });
  // The surface fades out on hand-off and reports its departure only after the shared fade duration.
  // 启动层在交接时淡出，并仅在共用淡出时长结束后报告离场。
  it("fades the startup surface out before reporting that it left", async () => {
    vi.useFakeTimers();
    try {
      // left records the owner's unmount signal.
      // left 记录持有者的卸载信号。
      const left = vi.fn();
      await act(async () => root.render(<BootLoadingScreen onLeft={left} />));
      expect(host.querySelector(".startup-surface")?.hasAttribute("data-leaving")).toBe(false);
      await act(async () => root.render(<BootLoadingScreen leaving onLeft={left} />));
      expect(host.querySelector(".startup-surface")?.getAttribute("aria-hidden")).toBe("true");
      await act(async () => vi.advanceTimersByTimeAsync(STARTUP_SURFACE_FADE_MS - 1));
      expect(left).not.toHaveBeenCalled();
      await act(async () => vi.advanceTimersByTimeAsync(1));
      expect(left).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
  });
  it("names an unknown-length download without inventing a numeric progress value", async () => {
    await act(async () => root.render(<StartupDownloadProgress label="Download" step={{
      id: "download", label: "Runtime", status: "running", progressPhase: "downloading", totalBytes: null,
    }} />));
    const progress = host.querySelector('[role="progressbar"]')!;
    expect(progress.getAttribute("aria-label")).toBe("Download");
    expect(progress.hasAttribute("aria-valuenow")).toBe(false);
  });
  it("keeps startup progress unknown before a real step percentage arrives", async () => {
    await act(async () => root.render(<StartupOverlay startup={{ ready: false, phase: "startup", recentLogs: [], steps: [
      { id: "done", label: "Done", status: "ready" }, { id: "active", label: "Active", status: "running" },
    ] }} />));
    expect(host.querySelector('[role="progressbar"]')?.hasAttribute("aria-valuenow")).toBe(false);
    await act(async () => root.render(<StartupOverlay startup={{ ready: false, phase: "startup", recentLogs: [], steps: [
      { id: "active", label: "Active", status: "running", progressPercent: 37 },
    ] }} />));
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("37");
  });
});
