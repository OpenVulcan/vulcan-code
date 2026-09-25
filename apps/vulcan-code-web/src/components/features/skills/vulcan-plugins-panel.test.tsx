// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { VulcanPluginsPanel } from "./vulcan-plugins-panel";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/service/core-browser-service", () => ({
  getCoreBrowserRuntimeStatus: vi.fn().mockResolvedValue({ ready: false }),
}));
// plugin is a complete disabled user-installed package without real runtime or external connections.
// plugin 是完整的已停用用户安装包，不使用真实运行时或外部连接。
const plugin: PluginSummary = { id: "fixture", name: "Fixture", kind: "plugin", status: "disabled", runtimeState: "disabled",
  capability: "Synthetic", nameByLocale: {}, capabilityByLocale: {}, enabled: false, desiredEnabled: false,
  available: true, toolCount: 0, tools: [], hookCount: 0, skillCount: 0, agentCount: 0, activeSessionCount: 0,
  diagnostics: [], managementKind: "vulcanPlugin", extension: { runtimeIdentity: "fixture:1", origin: "userInstalled", permissions: [], hooks: [],
    actions: [], uiMounts: [], skills: [], agents: [], hasLuaRuntime: false, mcpServers: [] } };
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("shows Playwright MCP and optional Core browser readiness independently", async () => {
  // browserPlugin reuses the complete fixture descriptor with the exact bundled browser identity.
  // browserPlugin 复用完整夹具描述符，并使用内置浏览器的精确身份。
  const browserPlugin = { ...plugin, id: "vulcan-browser" };
  await act(async () => root.render(<VulcanPluginsPanel plugins={[browserPlugin]} actions={{
    onPluginEnvironmentReady: async () => true,
    onPluginEnvironmentInstallStatusForPlugin: async () => null,
  } as unknown as SkillActionProps} />));
  expect(host.textContent).toContain("Playwright MCP dependencies: installed; Core browser component: missing or unavailable");
});

it("requires shared confirmation before uninstall and keeps failed confirmation retryable", async () => {
  const uninstall = vi.fn().mockRejectedValueOnce(new Error("busy runtime")).mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onUninstallUserPlugin: uninstall, onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null } as unknown as SkillActionProps} />));
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')!.click());
  expect(uninstall).not.toHaveBeenCalled();
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog.textContent).toContain("configuration and isolated environment");
  const confirm = [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Confirm")!;
  await act(async () => confirm.click());
  expect(dialog.textContent).toContain("busy runtime");
  expect(confirm.disabled).toBe(false);
  await act(async () => confirm.click());
  expect(uninstall).toHaveBeenCalledTimes(2);
  expect(uninstall).toHaveBeenLastCalledWith("fixture");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("shows a lifecycle failure and clears it when the same action succeeds", async () => {
  const toggle = vi.fn().mockRejectedValueOnce(new Error("activation failed")).mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onSetVulcanPluginEnabled: toggle, onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null } as unknown as SkillActionProps} />));
  const control = host.querySelector<HTMLButtonElement>('[role="switch"]')!;
  await act(async () => control.click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("activation failed");
  await act(async () => control.click());
  expect(toggle).toHaveBeenCalledTimes(2);
  expect(host.querySelector('[role="alert"]')).toBeNull();
});

it("keeps enable unavailable until the explicit environment install succeeds", async () => {
  // task remains owned by Core while the panel receives only read-only progress.
  // task 始终由 Core 持有，面板只接收只读进度。
  const runningTask = { taskId: "task-1", pluginId: "fixture", generation: "generation-1", state: "running" as const, stage: "preparing" as const };
  const completedTask = { ...runningTask, state: "succeeded" as const, stage: "completed" as const };
  let completeTask!: (task: typeof completedTask) => void;
  const status = vi.fn().mockImplementation(() => new Promise<typeof completedTask>((resolve) => { completeTask = resolve; }));
  const start = vi.fn().mockResolvedValue(runningTask);
  let ready = false;
  // toggle records the separate enable request after environment verification.
  // toggle 记录环境校验后的独立启用请求。
  const toggle = vi.fn().mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{
    onPluginEnvironmentReady: async () => ready,
    onPluginEnvironmentInstallStatusForPlugin: async () => null,
    onStartPluginEnvironmentInstall: start,
    onPluginEnvironmentInstallStatus: status,
    onSetVulcanPluginEnabled: toggle,
  } as unknown as SkillActionProps} />));
  // control is disabled while Core reports that the current generation has no environment.
  // control 在 Core 报告当前代次没有环境时保持禁用。
  const control = host.querySelector<HTMLButtonElement>('[role="switch"]')!;
  expect(control.disabled).toBe(true);
  // installButton is the explicit one-click environment preparation action.
  // installButton 是显式的一键环境准备动作。
  const installButton = [...host.querySelectorAll("button")].find((button) => button.textContent === "Install runtime")!;
  await act(async () => installButton.click());
  expect(start).toHaveBeenCalledExactlyOnceWith("fixture");
  expect(toggle).not.toHaveBeenCalled();
  expect(control.disabled).toBe(true);
  expect(host.textContent).toContain("Install stage: preparing");
  ready = true;
  await act(async () => {
    completeTask(completedTask);
    await new Promise((resolve) => window.setTimeout(resolve, 0));
  });
  expect(control.disabled).toBe(false);
  await act(async () => control.click());
  expect(toggle).toHaveBeenCalledExactlyOnceWith("fixture", true);
});

it("resumes a Core-owned install task after the panel mounts", async () => {
  // latest is queried from Core so an installation started by another client remains visible.
  // latest 从 Core 查询，因此其他客户端启动的安装仍然可见。
  const latest = vi.fn().mockResolvedValue({ taskId: "foreign-1", pluginId: "fixture", generation: "generation-1", state: "failed", stage: "failed", error: "registry unavailable" });
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{
    onPluginEnvironmentReady: async () => false,
    onPluginEnvironmentInstallStatusForPlugin: latest,
  } as unknown as SkillActionProps} />));
  expect(latest).toHaveBeenCalledWith("fixture");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("registry unavailable");
  expect(host.textContent).toContain("Install runtime");
});
