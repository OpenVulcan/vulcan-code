// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { formatMessage, readLocaleMessage, type TranslationKey } from "@/i18n";
import { readPluginResourceDiagnostics, type PluginDiagnosticsObservation } from "@/service/plugin-resource-diagnostics-service";
import { diagnosticReport, diagnosticResource } from "@/service/__fixtures__/plugin-resource-diagnostics";
import type { WorkbenchSnapshot } from "@/types";
import type { PluginDiagnosticsSelection } from "@/types/plugin-resource-diagnostics";
import { PluginResourceDiagnostics } from "./plugin-resource-diagnostics";

vi.mock("@/service/plugin-resource-diagnostics-service", () => ({ readPluginResourceDiagnostics: vi.fn() }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual preserves the production catalog and exact translated keys.
  // actual 保留生产语言目录及精确翻译键。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ t: (key: TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});
// host and root own the actual DOM for asynchronous navigation tests.
// host 与 root 拥有异步导航测试的实际 DOM。
let host: HTMLDivElement;
// root is disposed after each independent observation test.
// root 在每个独立观测测试后销毁。
let root: Root;
// selectConversation records the exact owner reaching the existing workspace callback boundary.
// selectConversation 记录到达既有工作台回调边界的精确所有者。
const selectConversation = vi.fn();
// select records only readonly diagnostic navigation.
// select 仅记录只读诊断导航。
const select = vi.fn();
// conversations uses a conversation identity deliberately distinct from host/core resource identities.
// conversations 使用刻意区别于宿主及核心资源身份的会话身份。
const conversations = [{ id: "conversation-original" }] as WorkbenchSnapshot["conversations"];
// report models a real safe response, preserving the independent pinned count.
// report 模拟实际安全响应，保留独立固定计数。
const report: PluginDiagnosticsObservation = { resources: diagnosticReport, update: null, uninstall: null };
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  selectConversation.mockResolvedValue(true);
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValue(report);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
// render selects the supplied original identity and returns after React effects complete.
// render 选择提供的原身份，在 React 效果完成后返回。
async function render(selection: PluginDiagnosticsSelection): Promise<void> {
  await act(async () => root.render(<PluginResourceDiagnostics selection={selection} onSelect={select}
    conversations={conversations} onSelectPluginResourceOwner={selectConversation} />));
}
// button resolves an actual translated semantic control; throws when its expected contract has changed.
// button 解析实际翻译语义控件；期望契约变化时抛错。
function button(key: TranslationKey): HTMLButtonElement {
  // found ignores evolving positions and locates the exact current visible label.
  // found 忽略演进位置，定位精确当前可见标签。
  const found = [...host.querySelectorAll("button")].find((candidate) => candidate.textContent === readLocaleMessage("en-US", key));
  if (!found) throw new Error(`Expected ${key}; label or availability may be outdated`);
  return found;
}
// en resolves the actual English message for key; values fill its placeholders.
// en 解析 key 的实际英文文案；values 填充其占位符。
function en(key: TranslationKey, values: Record<string, string | number> = {}): string { return formatMessage(readLocaleMessage("en-US", key), values); }
// stat reads one summary tile's number by its label, independently from tile order.
// stat 按标签读取单个汇总卡片的数字，独立于卡片顺序。
function stat(key: TranslationKey): string | undefined {
  return [...host.querySelectorAll(".vk-page-facts[data-layout='tiles'] > div")].find((tile) => tile.querySelector("dt")?.textContent === en(key))?.querySelector("dd")?.textContent ?? undefined;
}

it("shows independent pinned count, retained terminal history, physical blockers and original error codes", async () => {
  await render({ kind: "plugin", pluginId: "fixture" });
  expect(stat("pluginDiagnostics.pinned")).toBe("1");
  expect(stat("pluginDiagnostics.stat.blocking")).toBe("2");
  expect(stat("pluginDiagnostics.stat.conversations")).toBe("1");
  // Terminal history is folded under its own disclosure, apart from live blockers.
  // 终态历史收在独立展开区，与实时阻塞项分开。
  expect(host.textContent).toContain(en("pluginDiagnostics.history", { count: 1 }));
  expect(host.textContent).toContain("retired-host-session");
  expect([...host.querySelectorAll(".vk-page-item[data-tone=warn]")]).toHaveLength(2);
  expect(host.textContent).toContain(en("pluginDiagnostics.blocking"));
  expect(host.textContent).toContain("cleanup_failed");
  expect(host.textContent).toContain("9007199254740993");
  expect(host.textContent).toContain(en("pluginDiagnostics.internal"));
  await act(async () => button("pluginDiagnostics.locate").click());
  expect(selectConversation).toHaveBeenCalledExactlyOnceWith("conversation-original", expect.any(AbortSignal));
  expect(selectConversation).not.toHaveBeenCalledWith(diagnosticResource.coreSessionId);
  expect(selectConversation).not.toHaveBeenCalledWith(diagnosticResource.resourceId);
});

it("retains the exact original request through refresh and keeps unavailable drains explicit", async () => {
  // original is a hidden plugin's stable request selected by its retained management row.
  // original 是保留管理行选中的隐藏插件稳定请求。
  const original: PluginDiagnosticsSelection = { kind: "update", pluginId: "hidden-plugin", requestId: "original-update" };
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValue({ resources: null, update: null, uninstall: null });
  await render(original);
  expect(host.textContent).toContain("original-update");
  expect(host.textContent).toContain(en("pluginDiagnostics.unavailable"));
  expect(host.textContent).not.toContain(en("pluginDiagnostics.empty"));
  await act(async () => button("pluginRuntime.refresh").click());
  expect(readPluginResourceDiagnostics).toHaveBeenLastCalledWith(original);
  expect(readPluginResourceDiagnostics).toHaveBeenCalledTimes(2);
});

it("does not navigate an archived original owner or invent a core conversation", async () => {
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValue({ ...report, resources: { ...diagnosticReport, resources: [{ ...diagnosticResource, ownerSessionId: "archived-owner" }] } });
  await render({ kind: "plugin", pluginId: "fixture" });
  expect(host.textContent).toContain("Original owner conversation unavailable or archived");
  expect([...host.querySelectorAll("button")].some((candidate) => candidate.textContent === readLocaleMessage("en-US", "pluginDiagnostics.locate"))).toBe(false);
  expect(selectConversation).not.toHaveBeenCalled();
});

it.each(["unavailable", "rejected"] as const)("shows exact owner selection %s while preserving the original resource evidence", async (failure) => {
  if (failure === "unavailable") selectConversation.mockResolvedValue(false);
  else selectConversation.mockRejectedValue(new Error("Selection service rejected"));
  await render({ kind: "plugin", pluginId: "fixture" });
  await act(async () => button("pluginDiagnostics.locate").click());
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("Original owner conversation unavailable or archived");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  if (failure === "rejected") expect(host.textContent).toContain("Selection service rejected");
});

it("invalidates the original owner's observation when its active conversation disappears while retaining diagnostic history", async () => {
  // original keeps the same diagnostics identity across the authoritative conversation lifecycle change.
  // original 在权威会话生命周期变化期间保持相同诊断身份。
  const original: PluginDiagnosticsSelection = { kind: "plugin", pluginId: "fixture" };
  // finish resolves the original renderer callback after its owner becomes unavailable.
  // finish 在原所有者不可用后完成原渲染器回调。
  let finish!: (selected: boolean) => void;
  selectConversation.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await render(original);
  await act(async () => button("pluginDiagnostics.locate").click());
  // observation belongs to the row and must be aborted by the active conversation projection changing.
  // observation 属于资源行，必须在活动会话投影变化时取消。
  const observation = selectConversation.mock.calls[0][1] as AbortSignal;
  expect(observation.aborted).toBe(false);
  await act(async () => root.render(<PluginResourceDiagnostics selection={original} onSelect={select}
    conversations={[]} onSelectPluginResourceOwner={selectConversation} />));
  expect(observation.aborted).toBe(true);
  expect(host.textContent).toContain("Original owner conversation unavailable or archived");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  await act(async () => finish(false));
  expect(host.querySelector('[role="alert"]')).toBeNull();
  expect(selectConversation).toHaveBeenCalledTimes(1);
});

it("rejects late previous selection results after a new original request is selected", async () => {
  // resolveOld completes a response after a newer request already owns the view.
  // resolveOld 在较新请求已拥有视图后完成响应。
  let resolveOld!: (value: PluginDiagnosticsObservation) => void;
  vi.mocked(readPluginResourceDiagnostics).mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
  await render({ kind: "plugin", pluginId: "old-plugin" });
  vi.mocked(readPluginResourceDiagnostics).mockResolvedValueOnce({ resources: null, update: null, uninstall: null });
  await render({ kind: "uninstall", pluginId: "new-plugin", requestId: "new-request" });
  await act(async () => resolveOld(report));
  expect(host.textContent).toContain("new-request");
  expect(host.textContent).not.toContain(diagnosticResource.resourceId);
  expect(host.textContent).not.toContain("old-plugin");
});

it("preserves original failures and stale evidence across cancelled refreshes and disposal", async () => {
  await render({ kind: "plugin", pluginId: "fixture" });
  vi.mocked(readPluginResourceDiagnostics).mockRejectedValueOnce(new Error("Original registry busy"));
  await act(async () => button("pluginRuntime.refresh").click());
  expect(host.textContent).toContain("Original registry busy");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  // finish completes the cancelled renderer observation without changing any service-owned work.
  // finish 完成已取消渲染器观测，不改变任何服务拥有的工作。
  let finish!: (value: PluginDiagnosticsObservation) => void;
  vi.mocked(readPluginResourceDiagnostics).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await act(async () => button("pluginRuntime.refresh").click());
  await act(async () => button("skills.plugins.cancel").click());
  await act(async () => finish({ resources: null, update: null, uninstall: null }));
  expect(host.textContent).toContain("Original registry busy");
  expect(host.textContent).toContain(diagnosticResource.resourceId);
  // pending finishes only after disposal and must never write a renderer state.
  // pending 仅在销毁后完成，绝不能写入渲染器状态。
  let pending!: (value: PluginDiagnosticsObservation) => void;
  vi.mocked(readPluginResourceDiagnostics).mockImplementationOnce(() => new Promise((resolve) => { pending = resolve; }));
  await act(async () => button("pluginRuntime.refresh").click());
  await act(async () => root.unmount());
  await act(async () => pending(report));
  // root is replaced only for the common cleanup hook after the intentional disposal assertion.
  // root 仅为刻意销毁断言后的共用清理钩子重新建立。
  root = createRoot(host);
});
