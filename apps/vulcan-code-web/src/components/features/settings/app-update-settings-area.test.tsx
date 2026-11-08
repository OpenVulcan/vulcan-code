// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppUpdateSettingsArea } from "./app-update-settings-area";
import type { AppUpdateSnapshot } from "@/service/app-update-events";

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
  installDesktopProfile: vi.fn(),
  saveAppUpdateSettings: vi.fn(),
} }));
vi.mock("@/service/app-update-service", () => service);
vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: { subscribe: vi.fn(async () => () => undefined) },
}));
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
    settings: { preview: false, route: "auto", checkInterval: "sixHours" },
    activity: "idle",
    progress: null,
    lastCheckedAt: null,
    offer: null,
    readyVersion: null,
    repairedFiles: null,
    error: null,
    rollback: null,
    desktopProfile: null,
    downloadPageUrl: "https://openvulcan.com/download",
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

// render mounts the page and settles its status read.
// render 挂载页面并完成其状态读取。
async function render(initial: AppUpdateSnapshot, serviceInstanceEpoch = 0): Promise<void> {
  service.loadAppUpdateStatus.mockResolvedValue(initial);
  await act(async () => { root.render(<AppUpdateSettingsArea serviceInstanceEpoch={serviceInstanceEpoch} />); });
}

// button finds the button whose label is the given catalog key.
// button 查找标签为给定目录键的按钮。
function button(key: string): HTMLButtonElement | undefined {
  return [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === key);
}

describe("AppUpdateSettingsArea", () => {
  it("reads the state of the new service after it reconnects", async () => {
    await render(snapshot({ currentVersion: "0.1.4", readyVersion: "0.1.5", activity: "restarting" }));
    await render(snapshot({ currentVersion: "0.1.5" }), 1);
    expect(service.loadAppUpdateStatus).toHaveBeenCalledTimes(2);
    expect(container.textContent).toContain("0.1.5");
    expect(button("settings.appUpdate.restart")).toBeUndefined();
  });

  it("offers no update actions when this copy cannot update itself", async () => {
    await render(snapshot({ unavailableReason: "notInstalled" }));
    expect(container.textContent).toContain("settings.appUpdate.unavailable.notInstalled");
    expect(button("settings.appUpdate.check")).toBeUndefined();
    expect(button("settings.appUpdate.repairAction")).toBeUndefined();
  });

  it("downloads an installable offer and shows the accepted state", async () => {
    await render(snapshot({ offer: {
      version: "0.1.5", channel: "stable", kind: "incremental", downloadBytes: 1024, releasedAt: "2026-11-04T00:00:00Z",
      notes: {}, skippedReleases: [], skippedReleasesComplete: true, installable: true, setupManaged: false, previouslyFailed: false,
    } }));
    service.downloadAppUpdate.mockResolvedValue(snapshot({ activity: "downloading", progress: { receivedBytes: 0, totalBytes: 1024 } }));
    await act(async () => { button("settings.appUpdate.download")?.click(); });
    expect(service.downloadAppUpdate).toHaveBeenCalledOnce();
    expect(container.querySelector("[data-app-update-progress]")).not.toBeNull();
    expect(button("settings.appUpdate.check")?.disabled).toBe(true);
  });

  it("sends a manual full update to the download page instead of downloading", async () => {
    await render(snapshot({ offer: {
      version: "0.2.0", channel: "stable", kind: "full", downloadBytes: 0, releasedAt: "2026-11-04T00:00:00Z",
      notes: {}, skippedReleases: [], skippedReleasesComplete: true, installable: false, setupManaged: false, previouslyFailed: false,
    } }));
    expect(button("settings.appUpdate.download")).toBeUndefined();
    expect(button("settings.appUpdate.openDownloadPage")).toBeDefined();
  });

  it("points a setup-managed installation to the setup program and offers no installer", async () => {
    await render(snapshot({ offer: {
      version: "0.2.0", channel: "stable", kind: "full", downloadBytes: 0, releasedAt: "2026-11-04T00:00:00Z",
      notes: {}, skippedReleases: [], skippedReleasesComplete: true, installable: false, setupManaged: true,
      previouslyFailed: false,
    } }));
    expect(button("settings.appUpdate.download")).toBeUndefined();
    expect(button("settings.appUpdate.openDownloadPage")).toBeUndefined();
    expect(container.querySelector("[data-app-update-offer]")?.textContent).toContain("settings.appUpdate.fullSetup");
  });

  it("downloads a new epoch that the setup program finishes like any other update", async () => {
    await render(snapshot({ offer: {
      version: "0.2.0", channel: "stable", kind: "setup", downloadBytes: 2048, releasedAt: "2026-11-04T00:00:00Z",
      notes: {}, skippedReleases: [], skippedReleasesComplete: true, installable: true, setupManaged: true,
      previouslyFailed: false,
    } }));
    expect(button("settings.appUpdate.openDownloadPage")).toBeUndefined();
    expect(container.querySelector("[data-app-update-offer]")?.textContent).toContain("settings.appUpdate.setup");
    service.downloadAppUpdate.mockResolvedValue(snapshot({ activity: "downloading", progress: { receivedBytes: 0, totalBytes: 2048 } }));
    await act(async () => { button("settings.appUpdate.download")?.click(); });
    expect(service.downloadAppUpdate).toHaveBeenCalledOnce();
  });

  it("installs a missing desktop profile and keeps the button off while the password prompt is open", async () => {
    const profile = { status: "missing" as const, installing: false, command: "sudo install -m 0644 a b", error: null };
    await render(snapshot({ desktopProfile: profile }));
    const notice = container.querySelector("[data-app-update-desktop-profile]");
    expect(notice?.textContent).toContain("sudo install -m 0644 a b");
    service.installDesktopProfile.mockResolvedValue(snapshot({ desktopProfile: { ...profile, installing: true } }));
    await act(async () => { button("settings.appUpdate.desktopProfile.install")?.click(); });
    expect(service.installDesktopProfile).toHaveBeenCalledOnce();
    expect(button("settings.appUpdate.desktopProfile.installing")?.disabled).toBe(true);
  });

  it("shows no desktop profile notice when the system needs none", async () => {
    await render(snapshot());
    expect(container.querySelector("[data-app-update-desktop-profile]")).toBeNull();
  });

  it("lists every version an update brings, newest first, below the settings", async () => {
    await render(snapshot({ offer: {
      version: "0.1.6", channel: "stable", kind: "incremental", downloadBytes: 1024, releasedAt: "2026-11-04T00:00:00Z",
      notes: { "en-US": "Six" }, skippedReleases: [{ version: "0.1.5", date: null, notes: { "en-US": "Five" } }],
      skippedReleasesComplete: true, installable: true, setupManaged: false, previouslyFailed: false,
    } }));
    const notes = [...container.querySelectorAll("[data-app-update-notes]")].map((element) => element.getAttribute("data-app-update-notes"));
    expect(notes).toEqual(["0.1.6", "0.1.5"]);
    const settings = container.querySelector("[role=switch]")!;
    const found = container.querySelector("[data-app-update-found]")!;
    expect(settings.compareDocumentPosition(found) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("saves a changed setting together with the unchanged one", async () => {
    await render(snapshot());
    service.saveAppUpdateSettings.mockResolvedValue(snapshot({ settings: { preview: true, route: "auto", checkInterval: "sixHours" } }));
    await act(async () => { container.querySelector<HTMLButtonElement>("[role=switch]")?.click(); });
    expect(service.saveAppUpdateSettings).toHaveBeenCalledWith({ preview: true, route: "auto", checkInterval: "sixHours" });
  });
});
