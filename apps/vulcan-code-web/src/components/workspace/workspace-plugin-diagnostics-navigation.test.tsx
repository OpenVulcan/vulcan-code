// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import type { PluginSummary, WorkbenchSnapshot } from "@/types";
import type { SkillActionProps } from "@/components/features/skills/skill-area-types";
import { diagnosticReport } from "@/service/__fixtures__/plugin-resource-diagnostics";
import { readPluginResourceDiagnostics } from "@/service/plugin-resource-diagnostics-service";
import { selectConversation } from "@/service/project-actions-service";
import { serviceClient } from "@/service/webui-service-runtime";
import { WorkspaceSkillsSettings } from "./workspace-skills-settings";

vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true, serviceClient: { request: vi.fn().mockResolvedValue(null) } }));
vi.mock("@/service/plugin-resource-diagnostics-service", () => ({ readPluginResourceDiagnostics: vi.fn() }));
vi.mock("@/components/features/skills/plugin-prewarm-controls", () => ({ PluginPrewarmControls: () => null }));
vi.mock("@/components/features/skills/plugin-prewarm-policy-controls", () => ({ PluginPrewarmPolicyControls: () => null }));
// plugin carries a real routing identity distinct from its package version.
// plugin 携带与包版本区分的真实路由身份。
const plugin: PluginSummary = { id: "fixture", name: "Fixture", kind: "plugin", status: "disabled", runtimeState: "disabled", version: "2.0.0",
  capability: "Synthetic", nameByLocale: {}, capabilityByLocale: {}, enabled: false, desiredEnabled: false, available: true,
  toolCount: 0, tools: [], hookCount: 0, skillCount: 0, agentCount: 0, activeSessionCount: 17, diagnostics: [], managementKind: "vulcanPlugin",
  extension: { runtimeIdentity: "epoch-original:9007199254740993", origin: "userInstalled", permissions: [], hooks: [], actions: [], uiMounts: [], skills: [], agents: [], hasLuaRuntime: true, mcpServers: [] } };
// workbench includes the original conversation owner projected by the workspace.
// workbench 包含工作台投影的原会话所有者。
const workbench = { plugins: [plugin], conversations: [{ id: "conversation-original" }], selectedConversationId: null } as WorkbenchSnapshot;
// actions keep mutation/recovery calls observable without dispatching production changes.
// actions 使变更及恢复调用可观测，不派发生产变更。
const actions = { onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null,
  onReadPluginUninstallRequests: vi.fn().mockResolvedValue([{ pluginId: "hidden-plugin", requestId: "original-uninstall", phase: "collection", requiresExternalEffectsAcknowledgement: false }]),
  onReadLocalPluginUpdates: vi.fn().mockResolvedValue([{ pluginId: "hidden-plugin", requestId: "original-update", phase: "published",
    candidate: { version: "3.0.0" }, installation: "succeeded", effects: [], hasServiceOwner: false }]),
  onResumeLocalPluginUpdate: vi.fn(), onAbortLocalPluginUpdate: vi.fn(), onResumePluginUninstallRequest: vi.fn(),
} as unknown as SkillActionProps;
// host owns the actual lazy workspace management DOM.
// host 拥有实际懒加载工作台管理 DOM。
let host: HTMLDivElement;
// root owns the full callback propagation through workspace settings, skills area and plugin panel.
// root 拥有穿过工作台设置、技能区域及插件面板的完整回调传递。
let root: Root;
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValue({ resources: diagnosticReport, update: null, uninstall: null });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><WorkspaceSkillsSettings activeSubArea="plugins" luaInstallDialogOpen={false}
    onLuaInstallDialogOpenChange={() => undefined} onOpenDiagnostics={() => undefined} skillActions={actions} workbench={workbench}
    onSelectPluginResourceOwner={async (conversationId) => (await selectConversation(conversationId))?.selectedConversationId === conversationId} /></I18nProvider>));
  await act(async () => { await import("@/components/features/skills/skills-area"); });
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
// click selects the semantically named button within its actual owner surface and returns after effects settle.
// click 在实际所属界面中选择语义命名按钮，在效果稳定后返回。
async function click(label: string, container: ParentNode = host): Promise<void> {
  // found ignores row positions and identifies the live control by its label.
  // found 忽略行位置，按标签标识实时控件。
  const found = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === label);
  if (!found) throw new Error(`Expected ${label}; the fixture label or availability may have changed`);
  await act(async () => found.click());
}

it("opens a real plugin's diagnostics and carries only the owner conversation to the existing selection RPC", async () => {
  // card is located by the stable plugin descriptor rather than an evolving list position.
  // card 根据稳定插件描述符定位，而非演进行位置。
  const card = [...host.querySelectorAll("article")].find((row) => row.textContent?.includes("Fixture"))!;
  expect(card.textContent).toContain("epoch-original:9007199254740993");
  expect(card.textContent).toContain("v2.0.0");
  expect(card.querySelector('[aria-label="Active calls and sessions 17"]')).not.toBeNull();
  await click(readLocaleMessage("en-US", "pluginDiagnostics.title"), card);
  expect(readPluginResourceDiagnostics).toHaveBeenCalledExactlyOnceWith({ kind: "plugin", pluginId: "fixture" });
  expect(host.textContent).toContain("Pinned tool sessions (not physically retired): 1");
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"));
  expect(serviceClient.request).toHaveBeenCalledExactlyOnceWith("service_select_conversation", { conversationId: "conversation-original" });
});

it("locates retained uninstall and update originals through the same diagnostics surface without recovery", async () => {
  await click(readLocaleMessage("en-US", "pluginRecovery.uninstallTitle"));
  // uninstall is selected using the original UUID even though its plugin has no card.
  // uninstall 使用原 UUID 选择，即使其插件没有卡片。
  const uninstall = [...host.querySelectorAll("article")].find((row) => row.textContent?.includes("original-uninstall"))!;
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locateRequest"), uninstall);
  expect(readPluginResourceDiagnostics).toHaveBeenLastCalledWith({ kind: "uninstall", pluginId: "hidden-plugin", requestId: "original-uninstall" });
  await click(readLocaleMessage("en-US", "pluginUpdates.title"));
  // input belongs to the update surface, identified by its actual catalog-generated label.
  // input 属于更新界面，由实际目录生成标签标识。
  const label = [...host.querySelectorAll("label")].find((candidate) => candidate.textContent === readLocaleMessage("en-US", "pluginUpdates.plugin"))!;
  // input uses its explicit label association rather than guessing a candidate node path.
  // input 使用明确标签关联，而非猜测候选节点路径。
  const input = document.getElementById(label.htmlFor) as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "hidden-plugin");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  // updateSurface is the original management input's owner, excluding the diagnostic read control.
  // updateSurface 是原管理输入所属区域，排除诊断读取控件。
  const updateSurface = input.closest("section")!;
  await click(readLocaleMessage("en-US", "pluginUpdates.read"), updateSurface);
  // update is located by the retained request UUID, never candidate version or array index.
  // update 根据保留请求 UUID 定位，绝不使用候选版本或数组下标。
  const update = [...host.querySelectorAll("article")].find((row) => row.textContent?.includes("original-update"))!;
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locateRequest"), update);
  expect(readPluginResourceDiagnostics).toHaveBeenLastCalledWith({ kind: "update", pluginId: "hidden-plugin", requestId: "original-update" });
  expect(actions.onResumePluginUninstallRequest).not.toHaveBeenCalled();
  expect(actions.onResumeLocalPluginUpdate).not.toHaveBeenCalled();
  expect(actions.onAbortLocalPluginUpdate).not.toHaveBeenCalled();
});
