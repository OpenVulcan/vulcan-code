// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { luaSkillsDebugInstallStatus, readLuaSkillsDebugStatus, startLuaSkillsDebugInstall } from "@/service/skills-actions-service";
import type { LuaSkillsDebugInstallTask, LuaSkillsDebugStatus } from "@/types";
import { LuaSkillsDebugPanel, runtimeSetupCommand } from "./lua-skills-debug-panel";

vi.mock("@/service/skills-actions-service", () => ({
  luaSkillsDebugInstallStatus: vi.fn(),
  readLuaSkillsDebugStatus: vi.fn(),
  startLuaSkillsDebugInstall: vi.fn(),
}));

// missing is a supported host with nothing installed yet.
// missing 是尚未安装任何版本的受支持宿主。
const missing: LuaSkillsDebugStatus = {
  series: "0.8",
  platform: "windows-x64",
  installRoot: "C:/vulcan/tools/luaskills-debug",
  version: null,
  compatible: false,
  executable: null,
  packageRoot: null,
  runtimeRoot: null,
  runtimeReady: false,
  task: null,
};
const installed: LuaSkillsDebugStatus = {
  ...missing,
  version: "0.8.1",
  compatible: true,
  executable: "C:/vulcan/tools/luaskills-debug/0.8.1/bin/luaskills-debug.exe",
  packageRoot: "C:/vulcan/tools/luaskills-debug/0.8.1",
  runtimeRoot: "C:/vulcan/tools/luaskills-debug/0.8.1/runtime",
};
const running: LuaSkillsDebugInstallTask = { taskId: "task-1", state: "running", phase: "downloading", completedBytes: 512, totalBytes: 1024, overallPercent: 40 };

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.useFakeTimers();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove();
  vi.useRealTimers(); vi.unstubAllGlobals(); vi.clearAllMocks();
});

async function mount() {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><LuaSkillsDebugPanel /></I18nProvider>));
  await act(async () => {});
}

it("installs in one click, follows the task by id and reads the status again when it ends", async () => {
  vi.mocked(readLuaSkillsDebugStatus).mockResolvedValueOnce(missing).mockResolvedValueOnce(installed);
  vi.mocked(startLuaSkillsDebugInstall).mockResolvedValue(running);
  vi.mocked(luaSkillsDebugInstallStatus).mockResolvedValue({ ...running, state: "succeeded", phase: "completed", overallPercent: 100 });
  await mount();
  expect(host.querySelector("[data-luaskills-debug-state]")?.getAttribute("data-luaskills-debug-state")).toBe("missing");
  await act(async () => host.querySelector<HTMLButtonElement>("[data-luaskills-debug-install]")!.click());
  expect(startLuaSkillsDebugInstall).toHaveBeenCalledOnce();
  expect(host.querySelector("[data-luaskills-debug-progress]")).not.toBeNull();
  expect(host.querySelector<HTMLButtonElement>("[data-luaskills-debug-install]")!.disabled).toBe(true);
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(luaSkillsDebugInstallStatus).toHaveBeenCalledWith("task-1");
  expect(readLuaSkillsDebugStatus).toHaveBeenCalledTimes(2);
  expect(host.querySelector("[data-luaskills-debug-state]")?.getAttribute("data-luaskills-debug-state")).toBe("ready");
  expect(host.querySelector("[data-luaskills-debug-progress]")).toBeNull();
  expect(host.querySelector("[data-luaskills-debug-setup]")).not.toBeNull();
});

it("resumes following an install that was already running when the page opened", async () => {
  vi.mocked(readLuaSkillsDebugStatus).mockResolvedValue({ ...missing, task: running });
  vi.mocked(luaSkillsDebugInstallStatus).mockResolvedValue(running);
  await mount();
  expect(host.querySelector("[data-luaskills-debug-progress]")).not.toBeNull();
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(luaSkillsDebugInstallStatus).toHaveBeenCalledWith("task-1");
  expect(startLuaSkillsDebugInstall).not.toHaveBeenCalled();
});

it("disables installing where luaskills-debug has no release", async () => {
  vi.mocked(readLuaSkillsDebugStatus).mockResolvedValue({ ...missing, platform: null, installRoot: null });
  await mount();
  expect(host.querySelector("[data-luaskills-debug-state]")?.getAttribute("data-luaskills-debug-state")).toBe("unsupported");
  expect(host.querySelector<HTMLButtonElement>("[data-luaskills-debug-install]")!.disabled).toBe(true);
});

it("builds the runtime setup command for the installed platform", () => {
  expect(runtimeSetupCommand(installed)).toContain("setup_runtime.ps1\" -Target lua");
  expect(runtimeSetupCommand({ ...installed, platform: "linux-x64", packageRoot: "/opt/luaskills-debug" })).toBe("bash \"/opt/luaskills-debug/setup_runtime.sh\" lua");
});
