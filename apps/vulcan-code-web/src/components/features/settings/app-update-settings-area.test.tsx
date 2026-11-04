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
async function render(initial: AppUpdateSnapshot): Promise<void> {
  service.loadAppUpdateStatus.mockResolvedValue(initial);
  await act(async () => { root.render(<AppUpdateSettingsArea />); });
}

// button finds the button whose label is the given catalog key.
// button 查找标签为给定目录键的按钮。
function button(key: string): HTMLButtonElement | undefined {
  return [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === key);
}

describe("AppUpdateSettingsArea", () => {
  it("offers no update actions when this copy cannot update itself", async () => {
    await render(snapshot({ unavailableReason: "notInstalled" }));
    expect(container.textContent).toContain("settings.appUpdate.unavailable.notInstalled");
    expect(button("settings.appUpdate.check")).toBeUndefined();
    expect(button("settings.appUpdate.repairAction")).toBeUndefined();
  });

  it("downloads an installable offer and shows the accepted state", async () => {
    await render(snapshot({ offer: {
      version: "0.1.5", channel: "stable", kind: "incremental", downloadBytes: 1024, releasedAt: "2026-11-04T00:00:00Z",
      notes: {}, installable: true, previouslyFailed: false,
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
      notes: {}, installable: false, previouslyFailed: false,
    } }));
    expect(button("settings.appUpdate.download")).toBeUndefined();
    expect(button("settings.appUpdate.openDownloadPage")).toBeDefined();
  });

  it("saves a changed setting together with the unchanged one", async () => {
    await render(snapshot());
    service.saveAppUpdateSettings.mockResolvedValue(snapshot({ settings: { preview: true, route: "auto" } }));
    await act(async () => { container.querySelector<HTMLButtonElement>("[role=switch]")?.click(); });
    expect(service.saveAppUpdateSettings).toHaveBeenCalledWith({ preview: true, route: "auto" });
  });
});
