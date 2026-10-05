// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { formatMessage, readLocaleMessage } from "@/i18n";
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
// en resolves the actual English copy for key, filling the fixture's display name.
// en 解析 key 的实际英文文案，并填入夹具显示名称。
const en = (key: import("@/i18n").TranslationKey) => formatMessage(readLocaleMessage("en-US", key), { name: plugin.name });
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // scrollIntoView is a browser API absent from jsdom; the usage view calls it when a selection changes.
  // scrollIntoView 是 jsdom 缺少的浏览器 API；使用情况视图在选择变化时调用它。
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
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
    onReadPluginUninstallRequests: async () => [],
  } as unknown as SkillActionProps} />));
  expect(host.textContent).toContain(formatMessage(readLocaleMessage("en-US", "skills.plugins.playwrightStatus"), {
    status: readLocaleMessage("en-US", "skills.plugins.playwrightReady"),
  }));
  expect(host.textContent).toContain(readLocaleMessage("en-US", "browser.builtInCoreBrowser"));
});

it("requires shared confirmation and discovers the original after uninstall failure without replay", async () => {
  // original is the sole actual service request returned after the initial response fails.
  // original 是初始响应失败后返回的唯一实际服务请求。
  const original = { pluginId: plugin.id, requestId: "12345678-1234-4234-8234-123456789abc", phase: "collection" as const,
    requiresExternalEffectsAcknowledgement: false };
  // read observes the registry on mount and then the actual retained original after failure.
  // read 在挂载时观测注册表，然后在失败后观测实际保留原实例。
  const read = vi.fn().mockResolvedValueOnce([]).mockResolvedValue([original]);
  // resume completes only the captured original UUID, never the ordinary uninstall entry.
  // resume 仅完成捕获原 UUID，绝不调用普通卸载入口。
  const resume = vi.fn().mockResolvedValue(undefined);
  // uninstall reports its foreground failure independently from the retained request observation.
  // uninstall 独立于保留请求观测报告前台失败。
  const uninstall = vi.fn().mockRejectedValueOnce(new Error("busy runtime")).mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onUninstallUserPlugin: uninstall,
    onReadPluginUninstallRequests: read, onResumePluginUninstallRequest: resume,
    onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null } as unknown as SkillActionProps} />));
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')!.click());
  expect(uninstall).not.toHaveBeenCalled();
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog.textContent).toContain("configuration and isolated environment");
  const confirm = [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Confirm")!;
  await act(async () => confirm.click());
  expect(host.textContent).toContain("busy runtime"); expect(host.textContent).toContain(en("pluginRecovery.uninstallUnknownNotice"));
  expect(host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')?.disabled).toBe(true);
  expect(read).toHaveBeenCalledTimes(2); expect(read).toHaveBeenLastCalledWith();
  expect(uninstall).toHaveBeenCalledTimes(1);
  expect(uninstall).toHaveBeenLastCalledWith("fixture");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  // The unconfirmed outcome opens the maintenance drawer and leaves a page-level reminder.
  // 未确认结果会打开维护抽屉，并在页面上留下提醒。
  expect(host.querySelector<HTMLElement>("aside.plugin-maintenance-drawer")?.hidden).toBe(false);
  expect(host.querySelector(".plugin-maintenance-attention")?.textContent).toContain(en("pluginMaintenance.attention.uncertain"));
  // recheck is the recovery center's only read; it observes the retained original without dispatching anything.
  // recheck 是恢复中心唯一的读取；它观测保留原实例，不派发任何操作。
  const recheck = [...host.querySelectorAll("button")].find((button) => button.textContent === en("pluginMaintenance.recovery.recheck"))!;
  await act(async () => recheck.click());
  expect(read).toHaveBeenCalledTimes(3); expect(host.textContent).toContain(original.requestId);
  // continueButton reviews the same retained original rather than dispatching a replacement.
  // continueButton 审阅相同保留原实例，而非派发替代请求。
  const continueButton = [...host.querySelectorAll("button")].find((button) => button.textContent === en("pluginRecovery.uninstallContinue"))!;
  await act(async () => continueButton.click()); read.mockResolvedValue([]);
  // finalConfirm belongs to the original request confirmation portal.
  // finalConfirm 属于原请求确认门户。
  const finalConfirm = [...document.querySelector('[role="alertdialog"]')!.querySelectorAll("button")].find((button) => button.textContent === "Confirm")!;
  await act(async () => finalConfirm.click());
  expect(resume).toHaveBeenCalledExactlyOnceWith(plugin.id, original.requestId, false);
  expect(uninstall).toHaveBeenCalledTimes(1);
});

// An empty read after a failed initial response never proves deletion or authorizes blind replay.
// 初始响应失败后的空读取绝不证明删除，也不授权盲重放。
it.each(["empty", "failed"] as const)("shows an initial no-ID uninstall result as unknown after a %s query and blocks ordinary replay", async (outcome) => {
  // uninstall loses its foreground response before an original can be discovered.
  // uninstall 在可发现原实例之前丢失前台响应。
  const uninstall = vi.fn().mockRejectedValue(new Error("response lost"));
  // read remains empty and cannot supply an authoritative completion receipt.
  // read 持续为空，不能提供权威完成回执。
  const read = vi.fn().mockResolvedValueOnce([]);
  if (outcome === "empty") read.mockResolvedValue([]);
  else read.mockRejectedValue(new Error("query unavailable"));
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onUninstallUserPlugin: uninstall,
    onReadPluginUninstallRequests: read, onPluginEnvironmentReady: async () => true,
    onPluginEnvironmentInstallStatusForPlugin: async () => null } as unknown as SkillActionProps} />));
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')!.click());
  // confirm is the ordinary uninstall confirmation, selected by its current semantic text.
  // confirm 是普通卸载确认，按当前语义文本选择。
  const confirm = [...document.querySelector('[role="alertdialog"]')!.querySelectorAll("button")].find((button) => button.textContent === "Confirm")!;
  await act(async () => confirm.click());
  expect(host.textContent).toContain(en("pluginMaintenance.attention.uncertain")); expect(host.textContent).toContain("response lost");
  expect(host.textContent).toContain(en("pluginRecovery.uninstallUnknownNotice"));
  expect(host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')?.disabled).toBe(true);
  expect(uninstall).toHaveBeenCalledTimes(1); expect(read).toHaveBeenCalledTimes(2);
  if (outcome === "failed") expect(host.textContent).toContain("query unavailable");
});

it("shows a lifecycle failure and clears it when the same action succeeds", async () => {
  const toggle = vi.fn().mockRejectedValueOnce(new Error("activation failed")).mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{ onSetVulcanPluginEnabled: toggle,
    onReadPluginUninstallRequests: async () => [], onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null } as unknown as SkillActionProps} />));
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
    onReadPluginUninstallRequests: async () => [],
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
    onReadPluginUninstallRequests: async () => [],
    onPluginEnvironmentInstallStatusForPlugin: latest,
  } as unknown as SkillActionProps} />));
  expect(latest).toHaveBeenCalledWith("fixture");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("registry unavailable");
  expect(host.textContent).toContain("Install runtime");
});

// Actual page recovery shares synchronous lifecycle admission without requiring a replay of ordinary actions.
// 实际页面恢复共享同步生命周期入场，无需重放普通动作。
it("holds the ordinary plugin controls closed during original installation recovery", async () => {
  // request is a retained original for this exact namespace, without credential values.
  // request 是此精确命名空间的保留原实例，不含凭据值。
  const request: import("@/service/plugin-install-recovery-service").PluginInstallStatus = {
    requestId: "12345678-1234-4234-8234-123456789abc", pluginId: plugin.id, phase: "lifecycle",
  };
  // finish resolves the same dispatched service-owned recovery after the page gate is inspected.
  // finish 在检查页面门之后结束相同已派发服务自有恢复。
  let finish!: () => void;
  // recover retains the exact explicit recovery call; it never dispatches install or enable.
  // recover 保留精确明确恢复调用；绝不派发安装或启用。
  const recover = vi.fn<(pluginId: string, requestId: string, acknowledgement: boolean) => Promise<void>>()
    .mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  // read observes the original until the service reports successful retirement.
  // read 观测原实例，直至服务报告成功退役。
  const read = vi.fn().mockResolvedValue([request]);
  // toggle proves the actual lifecycle control never dispatches while recovery owns admission.
  // toggle 证明恢复拥有入场期间实际生命周期控件绝不派发。
  const toggle = vi.fn().mockResolvedValue(undefined);
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{
    onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null,
    onReadPluginUninstallRequests: async () => [], onReadLocalPluginUpdates: async () => [], onReadPluginSecretMutations: async () => [],
    onReadPluginInstallRequests: read, onRecoverPluginInstallRequest: recover, onSetVulcanPluginEnabled: toggle,
  } as unknown as SkillActionProps} />));
  // find locates one actual visible English button or reports current fixture drift.
  // find 定位单个实际可见英文按钮，或报告当前夹具漂移。
  const find = (text: string): HTMLButtonElement => {
    // matched is selected semantically across the actual confirmation portal.
    // matched 跨实际确认门户按语义选择。
    const matched = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent === text);
    if (!matched) throw new Error(`Expected ${text}; the control label or availability may have changed`);
    return matched;
  };
  // The card's maintenance menu opens this plugin's recovery, which reads its retained originals on its own.
  // 卡片维护菜单打开此插件的恢复检查，它会自行读取保留原实例。
  const menu = host.querySelector<HTMLButtonElement>(`button[aria-label="${en("pluginMaintenance.menu")}"]`);
  if (!menu) throw new Error("Expected the plugin card's maintenance menu; its label may have changed");
  await act(async () => menu.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((candidate) => candidate.textContent === en("pluginMaintenance.action.recovery"));
  if (!item) throw new Error("Expected the recovery menu item; its label may have changed");
  await act(async () => item.click());
  expect(read).toHaveBeenCalledWith(plugin.id); expect(recover).not.toHaveBeenCalled();
  await act(async () => find(en("pluginRecovery.action.rollbackInstall")).click());
  await act(async () => find("Confirm").click());
  // control is the actual ordinary plugin toggle, not a test-only admission observer.
  // control 是实际普通插件开关，而非测试专用入场观察者。
  const control = host.querySelector<HTMLButtonElement>('[role="switch"]');
  if (!control) throw new Error("Expected ordinary plugin toggle; its contract may have changed");
  expect(control.disabled).toBe(true);
  await act(async () => control.click()); expect(toggle).not.toHaveBeenCalled();
  read.mockResolvedValue([]); await act(async () => finish());
  expect(control.disabled).toBe(false);
  expect(recover).toHaveBeenCalledExactlyOnceWith(plugin.id, request.requestId, false);
});

// The real page gate also excludes lifecycle and ordinary uninstall while an original deletion continues.
// 真实页面门在原删除继续期间也排除生命周期及普通卸载。
it("holds ordinary controls closed during exact uninstall continuation without repeat dispatch", async () => {
  // original is the retained deletion read from the service on page mount.
  // original 是页面挂载时从服务读取的保留删除。
  const original = { pluginId: plugin.id, requestId: "12345678-1234-4234-8234-123456789abc", phase: "collection" as const,
    requiresExternalEffectsAcknowledgement: false };
  // finish resolves the one actual-shaped continuation already dispatched by this page.
  // finish 结束此页面已派发的唯一符合实际结构继续。
  let finish!: () => void;
  // resume records the original continuation while keeping the page admission occupied.
  // resume 记录原继续，并保持页面入场占用。
  const resume = vi.fn().mockImplementationOnce(() => new Promise<void>((resolve) => { finish = resolve; }));
  // read removes the original only after the foreground response is acknowledged.
  // read 仅在前台响应已确认之后移除原实例。
  const read = vi.fn().mockResolvedValue([original]);
  // toggle and uninstall observe ordinary actions, which must never dispatch during continuation.
  // toggle 及 uninstall 观测普通动作，继续期间绝不应派发。
  const toggle = vi.fn();
  // uninstall records replacement entry calls independently from original continuation.
  // uninstall 独立于原继续记录替代入口调用。
  const uninstall = vi.fn();
  await act(async () => root.render(<VulcanPluginsPanel plugins={[plugin]} actions={{
    onReadPluginUninstallRequests: read, onResumePluginUninstallRequest: resume,
    onUninstallUserPlugin: uninstall, onSetVulcanPluginEnabled: toggle,
    onPluginEnvironmentReady: async () => true, onPluginEnvironmentInstallStatusForPlugin: async () => null,
  } as unknown as SkillActionProps} />));
  // find selects the current semantic label rather than a shifting control index.
  // find 选择当前语义标签，而非移动控件下标。
  const find = (text: string): HTMLButtonElement => {
    // found is the actual ordinary or recovery control.
    // found 是实际普通或恢复控件。
    const found = [...document.querySelectorAll("button")].find((button) => button.textContent === text);
    if (!found) throw new Error(`Expected ${text}; the current label or availability may have changed`);
    return found;
  };
  // The retained original is announced on the page; its review button opens the maintenance drawer.
  // 保留原实例在页面上提示；其处理按钮打开维护抽屉。
  expect(host.querySelector(".plugin-maintenance-attention")?.textContent).toContain(
    formatMessage(readLocaleMessage("en-US", "pluginMaintenance.attention.retained"), { count: 1 }));
  await act(async () => find(en("pluginMaintenance.attention.open")).click());
  expect(host.querySelector<HTMLElement>("aside.plugin-maintenance-drawer")?.hidden).toBe(false);
  await act(async () => find(en("pluginRecovery.uninstallContinue")).click());
  await act(async () => find("Confirm").click());
  // control and remove are real ordinary plugin controls guarded by the parent's synchronous admission.
  // control 及 remove 是受父级同步入场保护的真实普通插件控件。
  const control = host.querySelector<HTMLButtonElement>('[role="switch"]')!;
  // remove is the original ordinary uninstall entry, never the continuation control.
  // remove 是原普通卸载入口，绝不是继续控件。
  const remove = host.querySelector<HTMLButtonElement>('button[aria-label="Uninstall user plugin"]')!;
  expect(control.disabled).toBe(true); expect(remove.disabled).toBe(true); expect(find("Confirm").disabled).toBe(true);
  await act(async () => { control.click(); remove.click(); find("Confirm").click(); });
  expect(toggle).not.toHaveBeenCalled(); expect(uninstall).not.toHaveBeenCalled(); expect(resume).toHaveBeenCalledTimes(1);
  read.mockResolvedValue([]); await act(async () => finish());
  expect(control.disabled).toBe(false); expect(remove.disabled).toBe(false);
  expect(host.querySelector(".plugin-maintenance-attention")).toBeNull();
  expect(resume).toHaveBeenCalledExactlyOnceWith(plugin.id, original.requestId, false);
});
