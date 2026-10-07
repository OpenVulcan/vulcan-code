// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-runtime-wire.json";
import { readLocaleMessage, type TranslationKey } from "@/i18n";
import { parseCapacityStatus, parsePluginRuntime, parseCapacityReceipt } from "@/service/plugin-runtime-contract";
import { executePluginRuntimeCommand, readPluginRuntime, type PluginRuntimeObservation } from "@/service/plugin-runtime-service";
import { PluginRuntimeControls } from "./plugin-runtime-controls";

vi.mock("@/service/plugin-runtime-service", () => ({ executePluginRuntimeCommand: vi.fn(), readPluginRuntime: vi.fn() }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual supplies the real catalog; only the selected locale is fixed for this fixture.
  // actual 提供真实文案目录；夹具仅固定所选语言。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "zh-CN", t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key) }) };
});

// host/root isolate actual shared components; observation is a mutable simulated server checkpoint.
// host/root 隔离真实共享组件；observation 为可变的模拟服务检查点。
let host: HTMLDivElement;
let root: Root;
let observation: PluginRuntimeObservation;
const refreshCatalog = vi.fn<() => Promise<void>>();

// m resolves the actual zh-CN label the component renders for key.
// m 解析组件为 key 渲染的实际 zh-CN 标签。
function m(key: TranslationKey): string { return readLocaleMessage("zh-CN", key); }

// button finds a semantic control by its exact visible label, never a changing structural index.
// button 按精确可见标签查找语义控件，不依赖可变化结构下标。
function button(key: TranslationKey): HTMLButtonElement {
  // match may live in a Radix portal outside the panel.
  // match 可能位于面板之外的 Radix 门户中。
  const match = [...document.querySelectorAll("button")].find((element) => element.textContent === m(key));
  if (!match) throw new Error(`Expected button "${m(key)}"; its text or availability may have changed`);
  return match;
}

// field resolves an associated control from label text and returns its actual DOM element.
// field 通过标签文本解析关联控件并返回真实 DOM 元素。
function field(key: TranslationKey): HTMLInputElement {
  // match identifies an explicit HTML label inside the portaled edit dialog.
  // match 标识门户编辑对话框内的明确 HTML 标签。
  const match = [...document.querySelectorAll("label")].find((element) => element.textContent === m(key));
  if (!match) throw new Error(`Expected field "${m(key)}"; its label may have changed`);
  return document.getElementById(match.htmlFor) as HTMLInputElement;
}

// menuAction opens the capacity row's overflow menu by keyboard and selects one item by its label.
// menuAction 以键盘打开容量行的更多操作菜单，并按标签选择单个菜单项。
async function menuAction(key: TranslationKey): Promise<void> {
  // trigger is named after the purpose it belongs to, independently from row order.
  // trigger 以其所属用途命名，独立于行顺序。
  const trigger = host.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]');
  if (!trigger) throw new Error("Expected the capacity overflow menu; its trigger may have changed");
  await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  const item = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((element) => element.textContent === m(key));
  if (!item) throw new Error(`Expected menu item "${m(key)}"; its label or availability may have changed`);
  expect(item.hasAttribute("data-disabled")).toBe(false);
  await act(async () => item.click());
}

// mount renders the panel; mounting performs the first read.
// mount 渲染面板；挂载即执行首次读取。
async function mount(): Promise<void> {
  await act(async () => root.render(<PluginRuntimeControls plugins={[]} onRefreshCatalog={refreshCatalog} />));
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  // scrollIntoView is a browser API absent from jsdom, not a replacement for Radix behavior.
  // scrollIntoView 是 jsdom 缺少的浏览器 API，而非对 Radix 行为的替代。
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  observation = {
    runtime: { ...parsePluginRuntime(wire.runtime)!, policyUpdate: null },
    catalog: { runtimeId: wire.catalog.runtimeId, registryRevision: wire.catalog.registryRevision, capacities: [parseCapacityStatus(wire.initialCapacity)] },
    receiptActions: [],
  };
  vi.mocked(readPluginRuntime).mockImplementation(async () => observation);
  vi.mocked(executePluginRuntimeCommand).mockResolvedValue();
  refreshCatalog.mockResolvedValue();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); Reflect.deleteProperty(Element.prototype, "scrollIntoView"); vi.unstubAllGlobals();
});

// Mounting only reads, and refreshing while editing must not rebase or overwrite the draft.
// 挂载仅读取；编辑时刷新不得重新设置基准或覆盖草稿。
it("reads on mount without writing and preserves a stale draft until explicitly discarded", async () => {
  await mount();
  expect(readPluginRuntime).toHaveBeenCalledTimes(1);
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  expect(host.querySelectorAll('[role="progressbar"]').length).toBeGreaterThan(0);
  await act(async () => button("pluginRuntime.edit").click());
  expect(field("pluginRuntime.queueCalls").value).toBe(String(wire.initialCapacity.snapshot.config.max_queued_calls));
  observation.catalog!.capacities = wire.catalog.capacities.map(parseCapacityStatus);
  await act(async () => button("pluginRuntime.refresh").click());
  expect(document.body.textContent).toContain(m("pluginRuntime.staleDraft"));
  expect(button("pluginRuntime.save").disabled).toBe(true);
  expect(field("pluginRuntime.queueCalls").value).toBe(String(wire.initialCapacity.snapshot.config.max_queued_calls));
  await act(async () => button("pluginRuntime.discardDraft").click());
  await act(async () => button("pluginRuntime.edit").click());
  expect(field("pluginRuntime.queueCalls").value).toBe(String(wire.runtime.policyUpdate.request.config.max_queued_calls));
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
});

// An uncertain mutation survives a refresh; an explicit retry uses its original UUID and predecessor.
// 不确定修改跨刷新保留；明确重试使用原 UUID 和前驱。
it("retains an uncertain submission across refresh and retries the identical request", async () => {
  await mount();
  await act(async () => button("pluginRuntime.edit").click());
  vi.mocked(executePluginRuntimeCommand).mockRejectedValueOnce(new Error("response lost"));
  await act(async () => button("pluginRuntime.save").click());
  // original records exactly what the user submitted before losing the response.
  // original 精确记录响应丢失前用户提交的内容。
  const original = vi.mocked(executePluginRuntimeCommand).mock.calls[0][0];
  expect(original.kind).toBe("update");
  expect(host.textContent).toContain("response lost");
  expect(host.textContent).toContain(m("pluginRuntime.unknown"));
  await act(async () => button("pluginRuntime.refresh").click());
  expect(readPluginRuntime).toHaveBeenCalledTimes(2);
  expect(executePluginRuntimeCommand).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(m("pluginRuntime.unknown"));
  await act(async () => button("pluginRuntime.retry").click());
  expect(vi.mocked(executePluginRuntimeCommand).mock.calls[1][0]).toBe(original);
});

// A global receipt belonging to another plugin must stay recoverable without selecting that plugin.
// 属于另一插件的全局回执必须可恢复，无须先选择该插件。
it("shows another plugin's retained transaction and requires explicit recovery", async () => {
  observation.runtime!.policyUpdate = { ...parseCapacityReceipt(wire.runtime.policyUpdate), phase: "prepared",
    appliedRevision: null, request: { ...parseCapacityReceipt(wire.runtime.policyUpdate).request, pluginId: "another-plugin" } };
  // The buttons are exactly the continuations the service declared for this receipt, in its order.
  // 按钮恰为服务为此回执声明的继续方式，并保持其顺序。
  observation.receiptActions = [{ action: "cancel", requiresAcknowledgement: false }, { action: "resume", requiresAcknowledgement: false }];
  await mount();
  expect(host.textContent).toContain("another-plugin");
  expect(button("pluginRuntime.edit").disabled).toBe(true);
  expect(() => button("pluginRuntime.ack")).toThrow();
  expect(button("pluginRuntime.cancelUpdate").disabled).toBe(false);
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  await act(async () => button("pluginRuntime.resume").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({ kind: "resume", updateId: wire.runtime.policyUpdate.request.updateId });
});

// Destructive capacity control requires an explicit review with the captured identity.
// 破坏性容量控制须明确核对捕获身份。
it("confirms exact closure and reloads the plugin list only on request", async () => {
  await mount();
  await menuAction("pluginRuntime.close");
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(m("pluginRuntime.closeWarning"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({ kind: "control", request: {
    runtimeId: wire.initialCapacity.runtimeId, capacityId: wire.initialCapacity.snapshot.capacity_id,
    expectedRevision: wire.initialCapacity.policyRevision, pluginId: wire.initialCapacity.snapshot.plugin_id,
    purpose: wire.initialCapacity.purpose, action: "close",
  } });
  expect(refreshCatalog).not.toHaveBeenCalled();
  await act(async () => button("pluginRuntime.refreshCatalog").click());
  expect(refreshCatalog).toHaveBeenCalledTimes(1);
});

// Inheritance reset must send an explicit null and preserve the identity captured before confirmation.
// 继承重置须发送明确空值，并保留确认之前捕获的身份。
it("confirms inheritance reset as an explicit null policy transaction", async () => {
  observation.catalog!.capacities = [{ ...parseCapacityStatus(wire.initialCapacity), budgetSource: "host_override" }];
  await mount();
  await menuAction("pluginRuntime.reset");
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(m("pluginRuntime.resetWarning"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({
    kind: "update", request: {
      runtimeId: wire.initialCapacity.runtimeId, capacityId: wire.initialCapacity.snapshot.capacity_id,
      expectedRevision: wire.initialCapacity.policyRevision, pluginId: wire.initialCapacity.snapshot.plugin_id,
      purpose: wire.initialCapacity.purpose, updateId: expect.any(String), config: null,
    },
  });
});
