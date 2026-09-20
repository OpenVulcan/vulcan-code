// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CoreBrowserRuntimeControls } from "./CoreBrowserRuntimeControls";
import { getCoreBrowserRuntimeStatus, installCoreBrowserRuntime, type CoreBrowserRuntimeStatus } from "@/service/core-browser-service";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN" }) }));
vi.mock("@/service/core-browser-service", () => ({ getCoreBrowserRuntimeStatus: vi.fn(), installCoreBrowserRuntime: vi.fn() }));
// host and root isolate the recovery UI from user configuration and real services.
// host 和 root 将恢复界面与用户配置及真实服务隔离。
let host: HTMLDivElement;
let root: Root;
// status models the authoritative Core result across remounts and installation attempts.
// status 模拟跨重新挂载及安装尝试的 Core 权威结果。
let status: CoreBrowserRuntimeStatus;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.clearAllMocks();
  status = { ready: false, canInstall: true, diagnostic: "Chromium missing", installation: null };
  vi.mocked(getCoreBrowserRuntimeStatus).mockImplementation(async () => status);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// button returns the exact text-labelled control from this fixture.
// button 返回此夹具中由文字标识的精确控件。
function button(label: string): HTMLButtonElement {
  return [...host.querySelectorAll("button")].find((element) => element.textContent === label)!;
}

it("offers explicit installation, joins Core progress, and switches to repair on success", async () => {
  await act(async () => root.render(<CoreBrowserRuntimeControls />));
  expect(installCoreBrowserRuntime).not.toHaveBeenCalled();
  expect(host.textContent).toContain("Chromium missing");
  vi.mocked(installCoreBrowserRuntime).mockResolvedValue({ taskId: "install-1", state: "running", error: null });
  await act(async () => button("安装／修复浏览器组件").click());
  expect(installCoreBrowserRuntime).toHaveBeenCalledTimes(1);
  expect(button("安装／修复浏览器组件").disabled).toBe(true);
  expect(host.textContent).toContain("正在下载并验证");
  status = { ...status, ready: true, diagnostic: null, installation: { taskId: "install-1", state: "succeeded", error: null } };
  await act(async () => button("刷新状态").click());
  expect(host.textContent).toContain("已就绪");
  expect(button("修复浏览器组件").disabled).toBe(false);
});

it("shows retained task failure after remount and allows retry", async () => {
  status.installation = { taskId: "failed-1", state: "failed", error: "Download timed out" };
  await act(async () => root.render(<CoreBrowserRuntimeControls />));
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("Download timed out");
  expect(button("安装／修复浏览器组件").disabled).toBe(false);
});

it("shows the distribution problem when this Core has no installer", async () => {
  status.canInstall = false;
  status.diagnostic = "Rebuild desktop runtime: installer missing";
  await act(async () => root.render(<CoreBrowserRuntimeControls />));
  expect(host.textContent).toContain("installer missing");
  expect(host.textContent).not.toContain("安装／修复浏览器组件");
  expect(button("刷新状态")).toBeDefined();
});
