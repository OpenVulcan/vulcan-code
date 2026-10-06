// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readLocaleMessage } from "@/i18n";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { PluginMaintenancePage } from "./plugin-maintenance-page";
import { PluginMaintenanceDrawer, usePluginMaintenanceDrawer } from "./plugin-maintenance-drawer";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
// Panels are replaced by mount probes; their own behavior is covered by their dedicated tests.
// 面板被替换为挂载探针；其自身行为由各自专属测试覆盖。
const mounted = vi.hoisted(() => [] as string[]);
vi.mock("./plugin-runtime-controls", () => ({ PluginRuntimeControls: () => { mounted.push("capacity"); return <p>capacity-probe</p>; } }));
vi.mock("./plugin-resource-diagnostics", () => ({ PluginResourceDiagnostics: () => { mounted.push("usage"); return <p>usage-probe</p>; } }));
vi.mock("./plugin-prewarm-policy-controls", () => ({ PluginPrewarmPolicyControls: () => { mounted.push("prewarm"); return <p>prewarm-probe</p>; } }));
vi.mock("./plugin-update-controls", () => ({ PluginUpdateControls: () => { mounted.push("recovery"); return <p>recovery-probe</p>; } }));
vi.mock("./plugin-uninstall-recovery-controls", () => ({ PluginUninstallRecoveryControls: () => null }));

// plugin is a content-only package, so it owns no Lua runtime capacity.
// plugin 是纯内容插件包，因此不拥有 Lua 运行时容量。
const plugin = { id: "content-only", name: "Content only", nameByLocale: {}, managementKind: "vulcanPlugin", extension: { hasLuaRuntime: false, origin: "userInstalled" } } as unknown as PluginSummary;
const gate = { mutationBusy: false, acquireMutation: () => true, releaseMutation: () => undefined };
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); mounted.length = 0;
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// tabLabels lists the visible maintenance tabs in their rendered order.
// tabLabels 按渲染顺序列出可见维护分页。
const tabs = () => [...host.querySelectorAll<HTMLButtonElement>(".page-tabs nav button")];
const tabLabels = () => tabs().map((tab) => tab.textContent);
// label resolves one maintenance tab title from the English catalog.
// label 从英文目录解析单个维护分页标题。
const label = (tab: "recovery" | "capacity" | "prewarm") => readLocaleMessage("en-US", `pluginMaintenance.tab.${tab}`);
// headings lists the rendered maintenance block titles.
// headings 列出已渲染的维护区块标题。
const headings = () => [...host.querySelectorAll(".page-section h3")].map((heading) => heading.textContent);

it("mounts each maintenance tab on first visit and keeps it alive afterwards", async () => {
  await act(async () => root.render(<PluginMaintenancePage actions={{} as SkillActionProps} conversations={[]} plugins={[plugin]} />));
  expect(tabLabels()).toEqual([label("recovery"), label("capacity"), label("prewarm")]);
  expect(mounted).toEqual(["recovery"]);
  // Every block explains when, how and what for, in plain language, without an extra click.
  // 每个区块无需额外点击即以平实语言说明何时、如何及用途。
  expect(host.textContent).toContain(readLocaleMessage("en-US", "pluginMaintenance.recovery.when"));
  // capacity is activated by an ordinary click on its page tab.
  // capacity 通过普通点击其页面分页激活。
  const capacity = tabs().find((tab) => tab.textContent === label("capacity"))!;
  await act(async () => capacity.click());
  expect(mounted).toContain("capacity");
  expect(host.textContent).toContain("recovery-probe");
  // The visited recovery panel stays mounted but hidden.
  // 已访问的恢复面板保持挂载但隐藏。
  expect(host.querySelectorAll<HTMLElement>(".page-tab-panel[hidden]")).toHaveLength(1);
});

// DrawerHarness opens the drawer for one plugin scope on mount.
// DrawerHarness 挂载时为单个插件范围打开抽屉。
function DrawerHarness(props: { scope: string }) {
  const drawer = usePluginMaintenanceDrawer();
  const { open } = drawer;
  React.useEffect(() => { open(props.scope, "usage"); }, [open, props.scope]);
  return <PluginMaintenanceDrawer actions={{} as SkillActionProps} conversations={[]} drawer={drawer} gate={gate}
    onUninstallConfirmed={() => undefined} pluginName={(pluginId) => pluginId} plugins={[plugin]} uninstallUncertainty={null} />;
}

it("shows recovery and usage but no runtime resources for a plugin without a Lua runtime", async () => {
  await act(async () => root.render(<DrawerHarness scope={plugin.id} />));
  const visible = host.querySelector<HTMLElement>('.plugin-maintenance-drawer-body > div:not([hidden])')!;
  expect([...visible.querySelectorAll(".page-section h3")].map((heading) => heading.textContent)).toEqual([
    readLocaleMessage("en-US", "pluginMaintenance.recovery.title"),
    readLocaleMessage("en-US", "pluginMaintenance.usage.title"),
  ]);
  expect(mounted).toContain("usage");
  expect(mounted).not.toContain("capacity");
  expect(headings()).not.toContain(readLocaleMessage("en-US", "pluginMaintenance.capacity.title"));
});
