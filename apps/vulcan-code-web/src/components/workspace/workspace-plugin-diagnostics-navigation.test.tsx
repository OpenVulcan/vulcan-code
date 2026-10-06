// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { formatMessage, I18nProvider, readLocaleMessage, type TranslationKey } from "@/i18n";
import type { PluginSummary, WorkbenchSnapshot } from "@/types";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/skill-area-types";
import { diagnosticReport } from "@/service/__fixtures__/plugin-resource-diagnostics";
import { readPluginResourceDiagnostics } from "@/service/plugin-resource-diagnostics-service";
import { selectConversation } from "@/service/project-actions-service";
import { serviceClient } from "@/service/webui-service-runtime";
import { WorkspaceSkillsSettings } from "./workspace-skills-settings";

vi.mock("@/service/webui-service-runtime", () => ({ hasServiceRuntime: () => true, serviceClient: { request: vi.fn().mockResolvedValue(null) } }));
vi.mock("@/service/plugin-resource-diagnostics-service", () => ({ readPluginResourceDiagnostics: vi.fn() }));
// plugin carries a real routing identity distinct from its package version.
// plugin 携带与包版本区分的真实路由身份。
const plugin: PluginSummary = { id: "fixture", name: "Fixture", kind: "plugin", runtimeState: "disabled", version: "2.0.0",
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
  // Updates are read per plugin, so only the unlisted owner reports its retained original.
  // 更新按插件读取，因此只有未列出的所有者报告其保留原实例。
  onReadLocalPluginUpdates: vi.fn(async (pluginId: string) => pluginId !== "hidden-plugin" ? [] : [{ pluginId, requestId: "original-update", phase: "published",
    candidate: { version: "3.0.0" }, installation: "succeeded", effects: [], hasServiceOwner: false }]),
  onReadPluginInstallRequests: vi.fn().mockResolvedValue([]), onReadPluginSecretMutations: vi.fn().mockResolvedValue([]),
  onResumeLocalPluginUpdate: vi.fn(), onAbortLocalPluginUpdate: vi.fn(), onResumePluginUninstallRequest: vi.fn(),
} as unknown as SkillActionProps;
// host owns the actual lazy workspace management DOM.
// host 拥有实际懒加载工作台管理 DOM。
let host: HTMLDivElement;
// root owns the full callback propagation through workspace settings, skills area and plugin panel.
// root 拥有穿过工作台设置、技能区域及插件面板的完整回调传递。
let root: Root;
// mount renders the actual workspace settings on one skills sub-area, waiting for the lazy area to resolve.
// mount 在单个技能子区域渲染实际工作台设置，并等待懒加载区域完成。
async function mount(subArea: SkillSubAreaId): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><WorkspaceSkillsSettings activeSubArea={subArea} luaInstallDialogOpen={false}
    onLuaInstallDialogOpenChange={() => undefined} onOpenDiagnostics={() => undefined} skillActions={actions} workbench={workbench}
    onSelectPluginResourceOwner={async (conversationId) => (await selectConversation(conversationId))?.selectedConversationId === conversationId} /></I18nProvider>));
  await act(async () => { await import("@/components/features/skills/skills-area"); });
}
// The lazy skills area is a large module graph; loading it once here keeps its cold transform out of every test's time budget.
// 懒加载技能区域是较大的模块图；在此统一加载一次，使其冷转换不计入每个测试的时间预算。
beforeAll(async () => { await import("@/components/features/skills/skills-area"); }, 60_000);
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValue({ resources: diagnosticReport, update: null, uninstall: null });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await mount("plugins");
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
// en resolves the actual English copy for key; values fill its placeholders.
// en 解析 key 的实际英文文案；values 填充其占位符。
const en = (key: TranslationKey, values: Record<string, string | number> = {}) => formatMessage(readLocaleMessage("en-US", key), values);
// click selects the semantically named button within its actual owner surface and returns after effects settle.
// click 在实际所属界面中选择语义命名按钮，在效果稳定后返回。
async function click(label: string, container: ParentNode = host): Promise<void> {
  // found ignores row positions and identifies the live control by its label.
  // found 忽略行位置，按标签标识实时控件。
  const found = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === label);
  if (!found) throw new Error(`Expected ${label}; the fixture label or availability may have changed`);
  await act(async () => found.click());
}

// openCardMenu opens the plugin card's maintenance menu through the keyboard path Radix supports in jsdom.
// openCardMenu 通过 Radix 在 jsdom 中支持的键盘路径打开插件卡片维护菜单。
async function openCardMenu(): Promise<void> {
  // trigger is named by the plugin it belongs to, never by its position in the card.
  // trigger 以所属插件命名，绝不依赖其在卡片中的位置。
  const trigger = host.querySelector<HTMLButtonElement>('button[aria-label="More actions for Fixture"]');
  if (!trigger) throw new Error("Expected the card maintenance menu; its label may have changed");
  await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })); });
}
// chooseMenuItem selects one open menu item by its exact visible label.
// chooseMenuItem 按精确可见标签选择一个已打开菜单项。
async function chooseMenuItem(label: string): Promise<void> {
  // item lives in the menu portal, outside the card itself.
  // item 位于菜单门户中，在卡片之外。
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((candidate) => candidate.textContent === label);
  if (!item) throw new Error(`Expected menu item ${label}; the menu contents may have changed`);
  await act(async () => item.click());
}
// drawer returns the maintenance side panel, which stays mounted while closed.
// drawer 返回维护侧边面板，关闭时仍保持挂载。
function drawer(): HTMLElement {
  const found = host.querySelector<HTMLElement>("aside.plugin-maintenance-drawer");
  if (!found) throw new Error("Expected the maintenance drawer; its structure may have changed");
  return found;
}
// stat reads one usage summary tile's number by its label, independently from tile order.
// stat 按标签读取单个使用情况汇总卡片的数字，独立于卡片顺序。
function stat(container: ParentNode, key: TranslationKey): string | undefined {
  return [...container.querySelectorAll(".page-facts[data-layout='tiles'] > div")].find((tile) => tile.querySelector("dt")?.textContent === en(key))
    ?.querySelector("dd")?.textContent ?? undefined;
}
// requestRow finds the recovery item whose technical details carry one original request UUID, never by list position.
// requestRow 按技术详情中的原请求 UUID 查找恢复条目，绝不依赖列表位置。
function requestRow(container: ParentNode, requestId: string): HTMLElement {
  const found = [...container.querySelectorAll<HTMLElement>("article")].find((row) => row.textContent?.includes(requestId));
  if (!found) throw new Error(`Expected a recovery item for ${requestId}; the fixture or row structure may have changed`);
  return found;
}

it("keeps maintenance off the card and opens a plugin's diagnostics from its menu", async () => {
  // card is located by the stable plugin descriptor rather than an evolving list position.
  // card 根据稳定插件描述符定位，而非演进行位置。
  const card = [...host.querySelectorAll("article")].find((row) => row.textContent?.includes("Fixture"))!;
  expect(card.textContent).not.toContain("epoch-original:9007199254740993");
  expect(card.textContent).not.toContain(en("pluginMaintenance.usage.title"));
  expect(card.textContent).toContain("v2.0.0");
  expect(card.querySelector('[aria-label="Active calls and sessions 17"]')).not.toBeNull();
  expect(drawer().hidden).toBe(true);
  await openCardMenu();
  await chooseMenuItem(en("pluginMaintenance.action.usage"));
  expect(drawer().hidden).toBe(false);
  expect(drawer().querySelector("h2")?.textContent).toBe(en("pluginMaintenance.scopedTitle", { name: "Fixture" }));
  expect(readPluginResourceDiagnostics).toHaveBeenCalledExactlyOnceWith({ kind: "plugin", pluginId: "fixture" });
  expect(stat(drawer(), "pluginDiagnostics.pinned")).toBe("1");
  await click(readLocaleMessage("en-US", "pluginDiagnostics.locate"), drawer());
  // The same drawer also reads the plugin's runtime capacity, so only conversation selections are counted here.
  // 同一抽屉还会读取插件运行时容量，因此此处仅统计会话选择请求。
  expect(vi.mocked(serviceClient.request).mock.calls.filter(([method]) => method === "service_select_conversation"))
    .toEqual([["service_select_conversation", { conversationId: "conversation-original" }]]);
  await act(async () => drawer().dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })));
  expect(drawer().hidden).toBe(true);
});

it("copies the runtime identity from the card menu instead of printing it on the card", async () => {
  // writeText observes the clipboard without touching the real system clipboard.
  // writeText 观测剪贴板，不触碰真实系统剪贴板。
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  await openCardMenu();
  await chooseMenuItem(readLocaleMessage("en-US", "pluginMaintenance.copyRuntimeIdentity"));
  expect(writeText).toHaveBeenCalledExactlyOnceWith("epoch-original:9007199254740993");
  expect(drawer().hidden).toBe(true);
});

it("surfaces retained uninstalls and locates originals through diagnostics without recovery", async () => {
  // attention is the only page-level trace of retained work; it names the count, not the request.
  // attention 是保留工作在页面上的唯一痕迹；它显示数量而非请求。
  const attention = host.querySelector("[data-attention='plugin-maintenance']");
  expect(attention?.textContent).toContain(en("pluginMaintenance.attention.retained", { count: 1 }));
  await click(en("pluginMaintenance.attention.open"), attention!);
  expect(drawer().hidden).toBe(false);
  expect(drawer().querySelector("h2")?.textContent).toBe(en("pluginMaintenance.uninstalls.title"));
  // uninstall is selected using the original UUID even though its plugin has no card.
  // uninstall 使用原 UUID 选择，即使其插件没有卡片。
  await click(en("pluginDiagnostics.locateRequest"), requestRow(drawer(), "original-uninstall"));
  expect(readPluginResourceDiagnostics).toHaveBeenLastCalledWith({ kind: "uninstall", pluginId: "hidden-plugin", requestId: "original-uninstall" });
  // The located usage opens beside the recovery item in the same drawer view.
  // 定位到的使用情况在同一抽屉视图中、恢复条目旁打开。
  expect(stat(drawer(), "pluginDiagnostics.pinned")).toBe("1");
  expect(actions.onResumePluginUninstallRequest).not.toHaveBeenCalled();
  expect(actions.onResumeLocalPluginUpdate).not.toHaveBeenCalled();
  expect(actions.onAbortLocalPluginUpdate).not.toHaveBeenCalled();
});

it("checks an unlisted plugin's update on the maintenance page and locates its original without recovery", async () => {
  await mount("plugin-maintenance");
  // page is the dedicated maintenance home; the unlisted owner has no card anywhere.
  // page 是专属维护页面；未列出的所有者在任何地方都没有卡片。
  const page = host.querySelector<HTMLElement>(".plugin-maintenance-page");
  if (!page) throw new Error("Expected the plugin maintenance page; its structure may have changed");
  expect(actions.onReadLocalPluginUpdates).not.toHaveBeenCalledWith("hidden-plugin");
  await click(en("pluginRecovery.extraToggle"), page);
  // extra is the explicit unlisted-owner field, identified by its associated label.
  // extra 是明确的未列出所有者字段，由其关联标签标识。
  const label = [...page.querySelectorAll("label")].find((candidate) => candidate.textContent === en("pluginRecovery.extraField"));
  if (!label) throw new Error("Expected the unlisted plugin field; its label may have changed");
  const extra = document.getElementById(label.htmlFor) as HTMLInputElement;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(extra, "hidden-plugin");
    extra.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await click(en("pluginRecovery.extraAdd"), extra.closest("form")!);
  expect(actions.onReadLocalPluginUpdates).toHaveBeenCalledWith("hidden-plugin");
  // update is located by the retained request UUID, never candidate version or array index.
  // update 根据保留请求 UUID 定位，绝不使用候选版本或数组下标。
  await click(en("pluginDiagnostics.locateRequest"), requestRow(page, "original-update"));
  expect(readPluginResourceDiagnostics).toHaveBeenLastCalledWith({ kind: "update", pluginId: "hidden-plugin", requestId: "original-update" });
  expect(stat(page, "pluginDiagnostics.pinned")).toBe("1");
  expect(actions.onResumePluginUninstallRequest).not.toHaveBeenCalled();
  expect(actions.onResumeLocalPluginUpdate).not.toHaveBeenCalled();
  expect(actions.onAbortLocalPluginUpdate).not.toHaveBeenCalled();
});
