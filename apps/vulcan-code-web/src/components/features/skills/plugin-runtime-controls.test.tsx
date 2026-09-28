// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-runtime-wire.json";
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

// button finds a semantic control by its exact visible label, never a changing structural index.
// button 按精确可见标签查找语义控件，不依赖可变化结构下标。
function button(label: string): HTMLButtonElement {
  // match may live in a Radix portal outside the panel.
  // match 可能位于面板之外的 Radix 门户中。
  const match = [...document.querySelectorAll("button")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected button "${label}"; its text or availability may have changed`);
  return match;
}

// field resolves an associated control from label text and returns its actual DOM element.
// field 通过标签文本解析关联控件并返回真实 DOM 元素。
function field(label: string): HTMLElement {
  // match identifies an explicit HTML label, including SelectTrigger associations.
  // match 标识明确 HTML 标签，包括 SelectTrigger 关联。
  const match = [...host.querySelectorAll("label")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected field "${label}"; its label may have changed`);
  return document.getElementById(match.htmlFor)!;
}

// openOwner opens the collapsed panel, then selects the retained owner through real Select keyboard/click events.
// openOwner 展开折叠面板，再经真实 Select 键盘及点击事件选择保留所有者。
async function openOwner(): Promise<void> {
  await act(async () => button("插件 VM 容量").click());
  await act(async () => field("插件").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  // owner is matched by native plugin identity rather than option ordering.
  // owner 按原生插件身份匹配，而非选项顺序。
  const owner = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((element) => element.textContent === wire.initialCapacity.snapshot.plugin_id);
  if (!owner) throw new Error("Expected retained plugin owner option; check catalog projection");
  await act(async () => owner.click());
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  // scrollIntoView is a browser API absent from jsdom, not a replacement for Select behavior.
  // scrollIntoView 是 jsdom 缺少的浏览器 API，而非对 Select 行为的替代。
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  observation = {
    runtime: { ...parsePluginRuntime(wire.runtime)!, policyUpdate: null },
    catalog: { runtimeId: wire.catalog.runtimeId, registryRevision: wire.catalog.registryRevision, capacities: [parseCapacityStatus(wire.initialCapacity)] },
  };
  vi.mocked(readPluginRuntime).mockImplementation(async () => observation);
  vi.mocked(executePluginRuntimeCommand).mockResolvedValue();
  refreshCatalog.mockResolvedValue();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<PluginRuntimeControls plugins={[]} onRefreshCatalog={refreshCatalog} />));
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); Reflect.deleteProperty(Element.prototype, "scrollIntoView"); vi.unstubAllGlobals();
});

// A closed panel is inert, and refreshing while editing must not rebase or overwrite the draft.
// 关闭面板不执行操作；编辑时刷新不得重新设置基准或覆盖草稿。
it("opens read-only and preserves a stale draft until explicitly discarded", async () => {
  expect(readPluginRuntime).not.toHaveBeenCalled();
  await openOwner();
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  await act(async () => button("编辑完整策略").click());
  expect((field("最大排队调用数") as HTMLInputElement).value).toBe(String(wire.initialCapacity.snapshot.config.max_queued_calls));
  observation.catalog!.capacities = wire.catalog.capacities.map(parseCapacityStatus);
  await act(async () => button("刷新运行时状态").click());
  expect(host.textContent).toContain("此草稿不能提交");
  expect(button("提交完整覆盖").disabled).toBe(true);
  expect((field("最大排队调用数") as HTMLInputElement).value).toBe(String(wire.initialCapacity.snapshot.config.max_queued_calls));
  await act(async () => button("丢弃草稿").click());
  await act(async () => button("编辑完整策略").click());
  expect((field("最大排队调用数") as HTMLInputElement).value).toBe(String(wire.runtime.policyUpdate.request.config.max_queued_calls));
});

// An uncertain mutation must survive collapse; an explicit retry uses its original UUID and predecessor.
// 不确定修改必须跨折叠保留；明确重试使用原 UUID 和前驱。
it("retains uncertain submission through collapse and retries the identical request", async () => {
  await openOwner();
  await act(async () => button("编辑完整策略").click());
  vi.mocked(executePluginRuntimeCommand).mockRejectedValueOnce(new Error("response lost"));
  await act(async () => button("提交完整覆盖").click());
  // original records exactly what the user submitted before losing the response.
  // original 精确记录响应丢失前用户提交的内容。
  const original = vi.mocked(executePluginRuntimeCommand).mock.calls[0][0];
  expect(original.kind).toBe("update");
  expect(host.textContent).toContain("response lost");
  await act(async () => button("插件 VM 容量").click());
  await act(async () => button("插件 VM 容量").click());
  expect(readPluginRuntime).toHaveBeenCalledTimes(1);
  await act(async () => button("重试原请求").click());
  expect(vi.mocked(executePluginRuntimeCommand).mock.calls[1][0]).toBe(original);
});

// A global receipt belonging to another plugin must stay recoverable without selecting that plugin.
// 属于另一插件的全局回执必须可恢复，无须先选择该插件。
it("shows another plugin's retained transaction and requires explicit recovery", async () => {
  observation.runtime!.policyUpdate = { ...parseCapacityReceipt(wire.runtime.policyUpdate), phase: "prepared",
    appliedRevision: null, request: { ...parseCapacityReceipt(wire.runtime.policyUpdate).request, pluginId: "another-plugin" } };
  await act(async () => button("插件 VM 容量").click());
  expect(host.textContent).toContain("another-plugin");
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  await act(async () => button("恢复原事务").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({ kind: "resume", updateId: wire.runtime.policyUpdate.request.updateId });
});

// Destructive capacity control requires an explicit review with the captured identity.
// 破坏性容量控制须明确核对捕获身份。
it("confirms exact closure and performs business publication only on request", async () => {
  await openOwner();
  await act(async () => button("关闭入场").click());
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("不保证物理排空");
  await act(async () => button("确认").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({ kind: "control", request: {
    runtimeId: wire.initialCapacity.runtimeId, capacityId: wire.initialCapacity.snapshot.capacity_id,
    expectedRevision: wire.initialCapacity.policyRevision, pluginId: wire.initialCapacity.snapshot.plugin_id,
    purpose: wire.initialCapacity.purpose, action: "close",
  } });
  expect(refreshCatalog).not.toHaveBeenCalled();
  await act(async () => button("刷新业务目录").click());
  expect(refreshCatalog).toHaveBeenCalledTimes(1);
});

// Inheritance reset must send an explicit null and preserve the identity captured before confirmation.
// 继承重置须发送明确空值，并保留确认之前捕获的身份。
it("confirms inheritance reset as an explicit null policy transaction", async () => {
  await openOwner();
  await act(async () => button("恢复继承").click());
  expect(executePluginRuntimeCommand).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("Core 将解析继承预算并校验");
  await act(async () => button("确认").click());
  expect(executePluginRuntimeCommand).toHaveBeenCalledWith({
    kind: "update", request: {
      runtimeId: wire.initialCapacity.runtimeId, capacityId: wire.initialCapacity.snapshot.capacity_id,
      expectedRevision: wire.initialCapacity.policyRevision, pluginId: wire.initialCapacity.snapshot.plugin_id,
      purpose: wire.initialCapacity.purpose, updateId: expect.any(String), config: null,
    },
  });
});
