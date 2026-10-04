// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-prewarm-wire.json";
import { parsePluginPrewarmCatalog, parsePluginPrewarmReceipt } from "@/service/plugin-prewarm-contract";
import { executePluginPrewarmCommand, readPluginPrewarmCatalog } from "@/service/plugin-prewarm-service";
import { readPluginRuntime, type PluginRuntimeObservation } from "@/service/plugin-runtime-service";
import type { ConversationSummary } from "@/types";
import type { PluginPrewarmCatalog } from "@/types/plugin-prewarm";
import { PluginPrewarmControls } from "./plugin-prewarm-controls";

vi.mock("@/service/plugin-runtime-service", () => ({ executePluginRuntimeCommand: vi.fn(), readPluginRuntime: vi.fn() }));
vi.mock("@/service/plugin-prewarm-service", () => ({ executePluginPrewarmCommand: vi.fn(), readPluginPrewarmCatalog: vi.fn() }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual supplies real localized labels while this test fixes the selected language.
  // actual 提供真实本地化标签，测试仅固定所选语言。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "zh-CN", t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key) }) };
});

// host/root own actual shared controls; catalog and parent simulate service checkpoints using real wire fixtures.
// host/root 持有真实共享控件；catalog 与 parent 使用真实线协议夹具模拟服务检查点。
let host: HTMLDivElement;
let root: Root;
let catalog: PluginPrewarmCatalog;
let parent: PluginRuntimeObservation;
const receipt = parsePluginPrewarmReceipt(wire.prepared);
const completed = parsePluginPrewarmReceipt(wire.completed);
const session: ConversationSummary = { id: receipt.ownerSessionId, title: "原会话", status: "idle" };

// button locates one visible semantic action by exact label and reports evolved labels clearly.
// button 按精确标签定位可见语义操作，并清晰报告标签变化。
function button(label: string): HTMLButtonElement {
  // match is resolved by meaning rather than structural position.
  // match 按语义解析，不依赖结构位置。
  const match = [...host.querySelectorAll("button")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected button ${label}; check current UI labels`);
  return match;
}

// field returns the element associated with label, including shared Select triggers.
// field 返回与 label 关联的元素，包括共享 Select 触发器。
function field(label: string): HTMLElement {
  // match requires an explicit label association, never an input ordinal.
  // match 要求明确标签关联，绝不使用输入框序号。
  const match = [...host.querySelectorAll("label")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected field ${label}; check current UI labels`);
  return document.getElementById(match.htmlFor)!;
}

// selectTool uses actual Select keyboard and option events to choose the original generation.
// selectTool 使用真实 Select 键盘与选项事件选择原代次。
async function selectTool(): Promise<void> {
  await act(async () => field("复用工具及代次").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  // option identifies the fixture's stable tool name within its displayed generation label.
  // option 根据显示的代次标签中的稳定工具名称定位夹具选项。
  const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((element) => element.textContent?.includes(receipt.toolId));
  if (!option) throw new Error("Expected reusable tool option; check actual declaration fixture");
  await act(async () => option.click());
}

// fillIdle dispatches a browser input event through React's actual controlled-input path.
// fillIdle 沿 React 真实受控输入链派发浏览器输入事件。
async function fillIdle(value: string): Promise<void> {
  // input and setter target the associated real Input, not component state internals.
  // input 与 setter 定位关联的真实 Input，不操作组件内部状态。
  const input = field("目标空闲 VM 数") as HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}

// openAndRead opens the inert panel and then performs the separately requested owner catalog read.
// openAndRead 展开初始无动作的面板，再执行单独请求的归属目录读取。
async function openAndRead(): Promise<void> {
  await act(async () => button("插件 VM 预热").click());
  await act(async () => button("读取预热目录与回执").click());
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  catalog = parsePluginPrewarmCatalog(wire.catalog);
  parent = { runtime: { runtimeId: catalog.runtimeId, resources: completed.satisfied!.physical, policyUpdate: null },
    catalog: { runtimeId: catalog.runtimeId, registryRevision: "registry-fixture", capacities: [] } };
  vi.mocked(readPluginRuntime).mockImplementation(async () => parent);
  vi.mocked(readPluginPrewarmCatalog).mockImplementation(async () => catalog);
  vi.mocked(executePluginPrewarmCommand).mockImplementation(async (command) => {
    // The mock advances only on explicit command, preserving the real fixture's original identity.
    // 模拟仅在明确操作时推进，保留真实夹具的原身份。
    if (command.kind === "prepare") { catalog = { ...catalog, targets: [receipt] }; return receipt; }
    if (command.kind === "forget") { catalog = { ...catalog, targets: [] }; return null; }
    catalog = { ...catalog, targets: [completed] }; return completed;
  });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<PluginPrewarmControls conversations={[session]} selectedConversationId={session.id} />));
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); Reflect.deleteProperty(Element.prototype, "scrollIntoView"); vi.unstubAllGlobals();
});

// Preparation and initialization require separate deliberate controls; completion is explicitly historical.
// 准备与初始化要求分别明确操作；完成观测明确标注为历史。
it("prepares then starts the exact original receipt without automatic initialization", async () => {
  expect(readPluginRuntime).not.toHaveBeenCalled();
  await openAndRead();
  expect(executePluginPrewarmCommand).not.toHaveBeenCalled();
  await selectTool();
  await fillIdle("3");
  expect(button("准备预热回执").disabled).toBe(true);
  await fillIdle("2");
  await act(async () => button("准备预热回执").click());
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
  expect(executePluginPrewarmCommand).toHaveBeenLastCalledWith({ kind: "prepare", request: {
    runtimeId: receipt.runtimeId, ownerSessionId: session.id, pluginId: receipt.pluginId, toolId: receipt.toolId,
    pluginGeneration: receipt.pluginGeneration, targetIdle: 2,
  } });
  expect(host.textContent).toContain(receipt.workspaceRoot);
  expect(button("遗忘已清理回执").disabled).toBe(true);
  await act(async () => button("启动原预热").click());
  expect(executePluginPrewarmCommand).toHaveBeenLastCalledWith({ kind: "start", request: {
    runtimeId: receipt.runtimeId, ownerSessionId: session.id, invocationId: receipt.invocationId,
  } });
  expect(host.textContent).toContain("历史观测，不代表当前或未来空闲量");
  expect(button("启动原预热").disabled).toBe(true);
  await act(async () => button("遗忘已清理回执").click());
  expect(host.textContent).toContain("此会话没有保留的预热回执");
});

// Automatic receipts remain visible and cancellable without authorizing manual replay of a host trigger.
// 自动回执保持可见及可取消，不授权手动重放宿主触发。
it("shows automatic provenance while refusing manual startup", async () => {
  catalog = { ...catalog, targets: [{ ...receipt, automatic: { triggerId: "original-turn", policyRevision: "original-policy" } }] };
  await openAndRead();
  expect(host.textContent).toContain("original-turn");
  expect(host.textContent).toContain("original-policy");
  expect(button("启动原预热").disabled).toBe(true);
  expect(button("取消原预热").disabled).toBe(false);
  await act(async () => button("启动原预热").click());
  expect(executePluginPrewarmCommand).not.toHaveBeenCalled();
  await act(async () => button("取消原预热").click());
  expect(executePluginPrewarmCommand).toHaveBeenLastCalledWith({ kind: "cancel", request: {
    runtimeId: receipt.runtimeId, ownerSessionId: receipt.ownerSessionId, invocationId: receipt.invocationId,
  } });
});

// Unknown preparation survives collapse and read recovery without manufacturing a retry button.
// 未知准备结果跨折叠及读取恢复保留，不产生重试按钮。
it("retains a lost preparation for inspection and never automatically repeats it", async () => {
  await openAndRead(); await selectTool(); await fillIdle("2");
  vi.mocked(executePluginPrewarmCommand).mockRejectedValueOnce(new Error("response lost"));
  await act(async () => button("准备预热回执").click());
  expect(host.textContent).toContain("请求结果未确认");
  expect(button("确认本地未知结果").disabled).toBe(true);
  await act(async () => button("插件 VM 预热").click());
  await act(async () => button("插件 VM 预热").click());
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
  // A late service receipt appears through the next read and remains inert until the user starts it.
  // 延迟服务回执通过下次读取出现，在用户启动之前保持未执行。
  catalog = { ...catalog, targets: [receipt] };
  await act(async () => button("读取预热目录与回执").click());
  expect(button("启动原预热").disabled).toBe(true);
  await act(async () => button("确认本地未知结果").click());
  expect(button("启动原预热").disabled).toBe(false);
  expect(executePluginPrewarmCommand).toHaveBeenCalledTimes(1);
});

// Navigation changes cannot reassign the prepared controller, and session removal preserves cleanup access.
// 导航变化不能重新指定已准备控制器；会话移除仍保留清理入口。
it("retains original session cleanup while disabling initialization after session removal", async () => {
  catalog = { ...catalog, targets: [receipt] };
  await openAndRead();
  const other = { ...session, id: "00000000-0000-4000-8000-000000000002", title: "其他会话" };
  await act(async () => root.render(<PluginPrewarmControls conversations={[other]} selectedConversationId={other.id} />));
  expect(field("所属会话").textContent).toContain("原会话");
  expect(host.textContent).toContain("此会话已不在活动列表");
  expect(button("启动原预热").disabled).toBe(true);
  expect(button("准备预热回执").disabled).toBe(true);
  await act(async () => button("读取预热目录与回执").click());
  expect(readPluginPrewarmCatalog).toHaveBeenLastCalledWith({ runtimeId: receipt.runtimeId, ownerSessionId: session.id });
  await act(async () => button("取消原预热").click());
  expect(executePluginPrewarmCommand).toHaveBeenLastCalledWith({ kind: "cancel", request: {
    runtimeId: receipt.runtimeId, ownerSessionId: session.id, invocationId: receipt.invocationId,
  } });
});

// Parent restarts invalidate all old actions rather than rebinding their invocation to the replacement.
// 父级重启使全部旧操作失效，不将旧调用重新绑定到替代父级。
it("does not reuse old receipts after observing a replacement parent", async () => {
  catalog = { ...catalog, targets: [receipt] };
  await openAndRead();
  parent = { runtime: { ...parent.runtime!, runtimeId: "replacement" }, catalog: { ...parent.catalog!, runtimeId: "replacement" } };
  await act(async () => button("刷新运行时状态").click());
  expect(host.querySelector(`article[aria-label="${receipt.invocationId}"]`)).toBeNull();
  expect(host.textContent).toContain("旧运行时的保留证据（仅供核查，不能转移到当前运行时）");
  expect(host.textContent).toContain(receipt.invocationId);
  expect(executePluginPrewarmCommand).not.toHaveBeenCalled();
});

// A removed controller remains recoverable from its exact UUID after all local selection state is lost.
// 全部本地选择状态丢失后，仍可通过精确 UUID 恢复已移除控制器的查询入口。
it("selects an absent original session explicitly without authorizing new initialization", async () => {
  await act(async () => root.render(<PluginPrewarmControls conversations={[]} selectedConversationId={null} />));
  await act(async () => button("插件 VM 预热").click());
  expect(button("读取预热目录与回执").disabled).toBe(true);
  // The original UUID is typed through the actual controlled input, not injected into hook state.
  // 原 UUID 通过真实受控输入输入，不注入钩子状态。
  const input = field("原会话 UUID（恢复查询）") as HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setter.call(input, "invalid"); input.dispatchEvent(new Event("input", { bubbles: true })); });
  await act(async () => button("按 UUID 选择会话").click());
  expect(readPluginPrewarmCatalog).not.toHaveBeenCalled();
  expect(button("读取预热目录与回执").disabled).toBe(true);
  await act(async () => { setter.call(input, session.id); input.dispatchEvent(new Event("input", { bubbles: true })); });
  await act(async () => button("按 UUID 选择会话").click());
  expect(readPluginPrewarmCatalog).not.toHaveBeenCalled();
  catalog = { ...catalog, targets: [{ ...receipt, started: true, requiresCleanupRecovery: true }] };
  await act(async () => button("读取预热目录与回执").click());
  expect(button("启动原预热").disabled).toBe(true);
  expect(button("准备预热回执").disabled).toBe(true);
  expect(button("恢复原清理").disabled).toBe(false);
  await act(async () => button("恢复原清理").click());
  expect(executePluginPrewarmCommand).toHaveBeenLastCalledWith({ kind: "recover", request: {
    runtimeId: receipt.runtimeId, ownerSessionId: session.id, invocationId: receipt.invocationId,
  } });
});
