// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { VulcanPluginsPanel } from "./vulcan-plugins-panel";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));
// plugin is a complete disabled user-installed package without real runtime or external connections.
// plugin 是完整的已停用用户安装包，不使用真实运行时或外部连接。
const plugin: PluginSummary = { id: "fixture", name: "Fixture", kind: "plugin", status: "disabled", runtimeState: "disabled",
  capability: "Synthetic", nameByLocale: {}, capabilityByLocale: {}, enabled: false, desiredEnabled: false,
  available: true, toolCount: 0, tools: [], hookCount: 0, skillCount: 0, agentCount: 0, activeSessionCount: 0,
  diagnostics: [], managementKind: "vulcanPlugin", extension: { origin: "userInstalled", permissions: [], hooks: [],
    actions: [], uiMounts: [], skills: [], agents: [], hasLuaRuntime: false, mcpServers: [] } };
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("requires shared confirmation before uninstall and keeps failed confirmation retryable", async () => {
  const uninstall = vi.fn().mockRejectedValueOnce(new Error("busy runtime")).mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onUninstallUserPlugin: uninstall } as unknown as SkillActionProps} />));
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
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onSetVulcanPluginEnabled: toggle } as unknown as SkillActionProps} />));
  const control = host.querySelector<HTMLButtonElement>('[role="switch"]')!;
  await act(async () => control.click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("activation failed");
  await act(async () => control.click());
  expect(toggle).toHaveBeenCalledTimes(2);
  expect(host.querySelector('[role="alert"]')).toBeNull();
});
