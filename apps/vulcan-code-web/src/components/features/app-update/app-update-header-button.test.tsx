// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppUpdateContext, type AppUpdateHost } from "./app-update-context";
import { AppUpdateHeaderButton } from "./app-update-header-button";
import type { AppUpdateOffer, AppUpdateSnapshot } from "@/service/app-update-events";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";

// service mocks every updater call, so no request leaves the test.
// service 模拟每个更新器调用，使任何请求都不会离开测试。
const { service } = vi.hoisted(() => ({ service: {
  loadAppUpdateStatus: vi.fn(),
  checkAppUpdate: vi.fn(),
  downloadAppUpdate: vi.fn(),
  cancelAppUpdate: vi.fn(),
  restartToUpdate: vi.fn(),
  repairAppInstall: vi.fn(),
  dismissAppUpdateNotice: vi.fn(),
  saveAppUpdateSettings: vi.fn(),
} }));
vi.mock("@/service/app-update-service", () => service);
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// snapshot builds an idle installed snapshot with the given overrides.
// snapshot 以给定覆盖值构建一个空闲的已安装快照。
function snapshot(overrides: Partial<AppUpdateSnapshot> = {}): AppUpdateSnapshot {
  return {
    currentVersion: "0.1.4",
    unavailableReason: null,
    settings: { preview: false, route: "auto" },
    activity: "idle",
    progress: null,
    lastCheckedAt: null,
    offer: null,
    readyVersion: null,
    repairedFiles: null,
    error: null,
    rollback: null,
    downloadPageUrl: "https://openvulcan.com/download",
    ...overrides,
  };
}

// offer builds an installable incremental offer whose English notes carry a marker.
// offer 构建一个可安装的增量提议，其英文说明带有标记文本。
function offer(overrides: Partial<AppUpdateOffer> = {}): AppUpdateOffer {
  return {
    version: "0.1.5", channel: "stable", kind: "incremental", downloadBytes: 1024, releasedAt: "2026-11-04T00:00:00Z",
    notes: { "en-US": "notes-marker-015" }, skippedReleases: [], skippedReleasesComplete: true,
    installable: true, previouslyFailed: false,
    ...overrides,
  };
}

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.clearAllMocks();
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

// render mounts the entry with a host holding the given snapshot and returns the host.
// render 以持有给定快照的宿主挂载入口并返回该宿主。
async function render(current: AppUpdateSnapshot | null, withHost = true): Promise<AppUpdateHost> {
  const host: AppUpdateHost = { snapshot: current, setSnapshot: vi.fn(), openSettings: vi.fn() };
  await act(async () => {
    root.render(<TooltipProvider>{withHost
      ? <AppUpdateContext.Provider value={host}><AppUpdateHeaderButton /></AppUpdateContext.Provider>
      : <AppUpdateHeaderButton />}</TooltipProvider>);
  });
  return host;
}

// entry returns the header update entry, if shown.
// entry 返回标题栏更新入口（如显示）。
function entry(): HTMLButtonElement | null {
  return container.querySelector<HTMLButtonElement>("[data-app-update-entry]");
}

// openDialog clicks the entry and waits for the lazily loaded dialog.
// openDialog 点击入口并等待懒加载的对话框。
async function openDialog(): Promise<HTMLElement> {
  await act(async () => { entry()?.click(); });
  // The first lazy import can exceed the default 1 s wait while many test files load in parallel.
  // 并行加载大量测试文件时，首次懒加载可能超过默认的 1 秒等待。
  await vi.waitFor(
    () => expect(document.querySelector("[data-app-update-dialog]")).not.toBeNull(),
    { timeout: 15_000 },
  );
  return document.querySelector<HTMLElement>("[data-app-update-dialog]")!;
}

// dialogButton finds a dialog button whose label is the given catalog key.
// dialogButton 查找标签为给定目录键的对话框按钮。
function dialogButton(dialog: HTMLElement, key: string): HTMLButtonElement | undefined {
  return [...dialog.querySelectorAll("button")].find((candidate) => candidate.textContent === key);
}

describe("AppUpdateHeaderButton", () => {
  it("shows nothing outside the workbench, before the state is known, or without an update", async () => {
    await render(snapshot({ offer: offer() }), false);
    expect(entry()).toBeNull();
    await render(null);
    expect(entry()).toBeNull();
    await render(snapshot());
    expect(entry()).toBeNull();
  });

  it("shows nothing when this copy cannot update itself", async () => {
    await render(snapshot({ unavailableReason: "notInstalled", offer: offer() }));
    expect(entry()).toBeNull();
  });

  it("previews the offered notes and downloads through the shared updater call", async () => {
    const host = await render(snapshot({ offer: offer() }));
    expect(entry()).not.toBeNull();
    const dialog = await openDialog();
    expect(dialog.querySelector("[data-app-update-notes]")?.textContent).toContain("notes-marker-015");
    const accepted = snapshot({ activity: "downloading", offer: offer(), progress: { receivedBytes: 0, totalBytes: 1024 } });
    service.downloadAppUpdate.mockResolvedValue(accepted);
    await act(async () => { dialogButton(dialog, "settings.appUpdate.download")?.click(); });
    expect(service.downloadAppUpdate).toHaveBeenCalledOnce();
    expect(host.setSnapshot).toHaveBeenCalledWith(accepted);
  });

  it("lists every skipped release after the offered one and says when earlier notes are missing", async () => {
    await render(snapshot({ offer: offer({
      version: "0.1.7",
      skippedReleases: [
        { version: "0.1.6", date: "2026-11-03", notes: { "en-US": "notes-marker-016" } },
        { version: "0.1.5", date: "2026-11-02", notes: { "en-US": "notes-marker-015" } },
      ],
      skippedReleasesComplete: false,
    }) }));
    const dialog = await openDialog();
    const sections = [...dialog.querySelectorAll<HTMLElement>("[data-app-update-notes]")];
    expect(sections.map((section) => section.dataset.appUpdateNotes)).toEqual(["0.1.7", "0.1.6", "0.1.5"]);
    expect(sections[1]?.textContent).toContain("notes-marker-016");
    expect(dialog.querySelector("[data-app-update-notes-incomplete]")).not.toBeNull();
  });

  it("keeps the notes after the download is ready and restarts through the shared call", async () => {
    const host = await render(snapshot({ offer: offer() }));
    await openDialog();
    // The Core drops the offer once the version is ready.
    // 版本就绪后 Core 会移除提议。
    await act(async () => {
      root.render(<TooltipProvider><AppUpdateContext.Provider value={{ ...host, snapshot: snapshot({ readyVersion: "0.1.5" }) }}>
        <AppUpdateHeaderButton />
      </AppUpdateContext.Provider></TooltipProvider>);
    });
    const dialog = document.querySelector<HTMLElement>("[data-app-update-dialog]")!;
    expect(dialog.querySelector("[data-app-update-notes]")?.textContent).toContain("notes-marker-015");
    expect(dialog.querySelector("[data-app-update-ready]")).not.toBeNull();
    service.restartToUpdate.mockResolvedValue(snapshot({ readyVersion: "0.1.5", activity: "restarting" }));
    await act(async () => { dialogButton(dialog, "settings.appUpdate.restart")?.click(); });
    expect(service.restartToUpdate).toHaveBeenCalledOnce();
  });

  it("opens the update settings page from the dialog", async () => {
    const host = await render(snapshot({ readyVersion: "0.1.5" }));
    expect(entry()).not.toBeNull();
    const dialog = await openDialog();
    await act(async () => { dialogButton(dialog, "appUpdate.dialog.openSettings")?.click(); });
    expect(host.openSettings).toHaveBeenCalledOnce();
    expect(document.querySelector("[data-app-update-dialog]")).toBeNull();
  });
});
