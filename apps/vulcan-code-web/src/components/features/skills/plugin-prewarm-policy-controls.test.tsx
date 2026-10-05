// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readLocaleMessage, type TranslationKey } from "@/i18n";
import wire from "@/service/__fixtures__/plugin-prewarm-policy-wire.json";
import { parsePluginPrewarmPolicyStatus } from "@/service/plugin-prewarm-policy-contract";
import { executePluginPrewarmPolicyCommand, readPluginPrewarmPolicyStatus } from "@/service/plugin-prewarm-policy-service";
import type { PluginSummary } from "@/types";
import type { PluginPrewarmPolicyStatus } from "@/types/plugin-prewarm-policy";
import { PluginPrewarmPolicyControls } from "./plugin-prewarm-policy-controls";

vi.mock("@/service/plugin-prewarm-policy-service", () => ({ executePluginPrewarmPolicyCommand: vi.fn(), readPluginPrewarmPolicyStatus: vi.fn() }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual retains production localization while selecting a deterministic UI language.
  // actual 保留生产本地化，同时选择确定的界面语言。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "zh-CN", t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key) }) };
});

// m resolves the production zh-CN label, so tests follow copy changes without hard-coded strings.
// m 解析生产 zh-CN 标签，使测试随文案变化而无需硬编码字符串。
const m = (key: TranslationKey) => readLocaleMessage("zh-CN", key);

// plugins offers two Lua packages so the rule editor cannot preselect one implicitly.
// plugins 提供两个 Lua 包，使规则编辑器无法隐式预选。
const plugins = [
  { id: "control-plugin", name: "Control", nameByLocale: {}, tools: [{ id: "control_tool" }], extension: { hasLuaRuntime: true } },
  { id: "other-plugin", name: "Other", nameByLocale: {}, tools: [], extension: { hasLuaRuntime: true } },
] as unknown as PluginSummary[];

// host and root mount actual shared controls; policy models complete service publication checkpoints.
// host 与 root 挂载真实共用控件；policy 模拟完整服务发布检查点。
let host: HTMLDivElement;
let root: Root;
let policy: PluginPrewarmPolicyStatus;
const published = parsePluginPrewarmPolicyStatus(wire.published);

// button finds an exact semantic action within scope and exposes missing or evolved labels clearly.
// button 在 scope 内查找精确语义操作，明确暴露缺失或演进的标签。
function button(label: string, scope: ParentNode = host): HTMLButtonElement {
  // match is located by its user-facing name, never by a structural ordinal.
  // match 根据用户可见名称定位，绝不依赖结构序号。
  const match = [...scope.querySelectorAll("button")].find((element) => element.textContent === label || element.getAttribute("aria-label") === label);
  if (!match) throw new Error(`Expected button ${label}; check current UI labels`);
  return match;
}

// control resolves the element bound to an explicit label inside scope.
// control 解析 scope 内与明确标签关联的元素。
function control<T extends HTMLElement>(label: string, scope: ParentNode = host): T {
  const associated = [...scope.querySelectorAll("label")].find((element) => element.textContent === label);
  if (!associated) throw new Error(`Expected field ${label}; check current UI labels`);
  return document.getElementById(associated.htmlFor) as T;
}

// fill changes the explicitly labelled controlled input inside scope through a real browser event.
// fill 通过真实浏览器事件修改 scope 内具有明确标签的受控输入。
async function fill(label: string, value: string, scope: ParentNode = host): Promise<void> {
  const input = control<HTMLInputElement>(label, scope);
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}

// choose opens the production Select by keyboard and picks the exact visible option.
// choose 通过键盘打开生产 Select 并选择准确的可见选项。
async function choose(label: string, option: string, scope: ParentNode = host): Promise<void> {
  const trigger = control<HTMLButtonElement>(label, scope);
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
  const entry = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((candidate) => candidate.textContent === option);
  if (!entry) throw new Error(`Expected option ${option}; check plugin fixtures`);
  await act(async () => { entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
}

// rows lists the rendered editor rows by their semantic class.
// rows 按语义类名列出已渲染编辑行。
const rows = () => [...host.querySelectorAll<HTMLElement>(".plugin-prewarm-editor-row")];

// mount renders the controls, which read the current policy once on their own.
// mount 渲染控件，控件自行读取一次当前策略。
async function mount(): Promise<void> {
  await act(async () => root.render(<PluginPrewarmPolicyControls plugins={plugins} />));
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  policy = parsePluginPrewarmPolicyStatus(wire.initial);
  vi.mocked(readPluginPrewarmPolicyStatus).mockImplementation(async () => policy);
  vi.mocked(executePluginPrewarmPolicyCommand).mockImplementation(async (command) => {
    if (command.kind === "forget") { policy = { ...policy, update: null }; return null; }
    // The mock publishes only on an explicit command, preserving its complete original request.
    // 模拟仅在明确操作时发布，保留完整原请求。
    const receipt = { ...published.update!, request: command.request };
    policy = { ...policy, revision: receipt.resultRevision, rules: command.request.rules, update: receipt };
    return receipt;
  });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

// Mounting only reads; the editor saves exact fields once and the receipt explains the outcome in plain words.
// 挂载只读取；编辑器只保存一次精确字段，回执以平实语言说明结果。
it("reads on mount, then edits and saves complete rules once", async () => {
  await mount();
  expect(readPluginPrewarmPolicyStatus).toHaveBeenCalledTimes(1);
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.disabled"));
  await act(async () => button(m("pluginPrewarmPolicy.add")).click());
  expect(button(m("pluginPrewarmPolicy.save")).disabled).toBe(true);
  await choose(m("pluginPrewarmPolicy.plugin"), "Control");
  await fill(m("pluginPrewarmPolicy.tool"), "control_tool"); await fill(m("pluginPrewarmPolicy.count"), "2");
  const save = button(m("pluginPrewarmPolicy.save"));
  await act(async () => { save.click(); save.click(); });
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "begin", request: {
    updateId: expect.any(String), runtimeId: wire.initial.runtimeId, expectedRevision: wire.initial.revision,
    rules: [{ pluginId: "control-plugin", toolId: "control_tool", targetIdle: 2 }],
  } });
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.receipt.committed"));
  expect(() => button(m("pluginPrewarmPolicy.cancel"))).toThrow();
  await act(async () => button(m("pluginPrewarmPolicy.forget")).click());
  expect(policy.update).toBeNull();
  expect(policy.rules).toEqual([{ pluginId: "control-plugin", toolId: "control_tool", targetIdle: 2 }]);
});

// Removing the last rule must submit an explicit empty collection rather than omitting the setting.
// 移除最后规则必须提交明确空集合，不能省略设置。
it("disables automatic policy by saving an explicit empty replacement", async () => {
  policy = { ...published, update: null }; await mount();
  expect(host.textContent).toContain("control_tool");
  await act(async () => button(m("pluginPrewarmPolicy.edit")).click());
  await act(async () => button(m("pluginPrewarmPolicy.remove")).click());
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.emptyDraft"));
  await act(async () => button(m("pluginPrewarmPolicy.save")).click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "begin", request: {
    updateId: expect.any(String), runtimeId: published.runtimeId, expectedRevision: published.revision, rules: [],
  } });
});

// Invalid counts and duplicate globally unique tool identities cannot reach the service.
// 无效数量及重复全局唯一工具身份不能到达服务。
it("rejects invalid counts and duplicate tool identities in the actual form", async () => {
  policy = { ...published, update: null }; await mount();
  await act(async () => button(m("pluginPrewarmPolicy.edit")).click());
  for (const invalid of ["", "0", "-1", "1.5", "01", "9007199254740992"]) {
    await fill(m("pluginPrewarmPolicy.count"), invalid);
    expect(button(m("pluginPrewarmPolicy.save")).disabled).toBe(true);
  }
  await fill(m("pluginPrewarmPolicy.count"), "1");
  expect(button(m("pluginPrewarmPolicy.save")).disabled).toBe(false);
  await act(async () => button(m("pluginPrewarmPolicy.addRow")).click());
  // The blank tool input identifies the newly added row without relying on its array position.
  // 空工具输入标识新增行，不依赖数组位置。
  const row = rows().find((candidate) => control<HTMLInputElement>(m("pluginPrewarmPolicy.tool"), candidate).value === "");
  if (!row) throw new Error("Expected a blank new rule row; check editor semantics");
  await choose(m("pluginPrewarmPolicy.plugin"), "Other", row);
  await fill(m("pluginPrewarmPolicy.tool"), "control_tool", row); await fill(m("pluginPrewarmPolicy.count"), "1", row);
  expect(button(m("pluginPrewarmPolicy.save")).disabled).toBe(true);
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
});

// A failed read keeps the last rules as evidence but blocks editing until a complete read succeeds.
// 读取失败时保留最后规则作为证据，但在完整读取成功前禁止编辑。
it("keeps stale rules visible and blocks edits until a fresh read", async () => {
  policy = { ...published, update: null }; await mount();
  vi.mocked(readPluginPrewarmPolicyStatus).mockRejectedValueOnce(new Error("offline"));
  await act(async () => button(m("pluginRuntime.refresh")).click());
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.stale"));
  expect(host.textContent).toContain("control_tool");
  expect(button(m("pluginPrewarmPolicy.edit")).disabled).toBe(true);
  await act(async () => button(m("pluginRuntime.refresh")).click());
  expect(button(m("pluginPrewarmPolicy.edit")).disabled).toBe(false);
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
});

// An unknown write is kept until explicitly inspected; acknowledgement never resends it.
// 未知写入保留至明确核查；确认绝不重发。
it("retains an unknown publication until a fresh read and acknowledgement", async () => {
  await mount();
  await act(async () => button(m("pluginPrewarmPolicy.add")).click());
  await choose(m("pluginPrewarmPolicy.plugin"), "Control");
  await fill(m("pluginPrewarmPolicy.tool"), "control_tool");
  vi.mocked(executePluginPrewarmPolicyCommand).mockRejectedValueOnce(new Error("response lost"));
  await act(async () => button(m("pluginPrewarmPolicy.save")).click());
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.unknown"));
  expect(button(m("pluginPrewarmPolicy.acknowledge")).disabled).toBe(true);
  expect(button(m("pluginPrewarmPolicy.add")).disabled).toBe(true);
  await act(async () => button(m("pluginRuntime.refresh")).click());
  expect(button(m("pluginPrewarmPolicy.add")).disabled).toBe(true);
  await act(async () => button(m("pluginPrewarmPolicy.acknowledge")).click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  expect(host.textContent).not.toContain(m("pluginPrewarmPolicy.unknown"));
  expect(button(m("pluginPrewarmPolicy.add")).disabled).toBe(false);
});

// Capacity retention and an uncertain final publication expose distinct restrictions in actual controls.
// 容量保留及未确认最终发布在真实控件中暴露不同限制。
it("shows capacity exclusion and protects publishing from cancellation or forgetting", async () => {
  policy = { ...policy, capacityUpdateRetained: true }; await mount();
  expect(button(m("pluginPrewarmPolicy.add")).disabled).toBe(true);
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.capacityBlocked"));
  policy = { ...published, update: { ...published.update!, phase: "publishing", persistenceError: "write confirmation lost" } };
  await act(async () => button(m("pluginRuntime.refresh")).click());
  expect(button(m("pluginPrewarmPolicy.resume")).disabled).toBe(false);
  expect(() => button(m("pluginPrewarmPolicy.cancel"))).toThrow();
  expect(() => button(m("pluginPrewarmPolicy.forget"))).toThrow();
  await act(async () => button(m("pluginPrewarmPolicy.resume")).click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "resume", request: published.update!.request });
});

// A prepared original transaction may be cancelled explicitly without manufacturing a new publication request.
// 已准备原事务可明确取消，不制造新的发布请求。
it("cancels the exact prepared receipt", async () => {
  policy = { ...policy, update: { ...published.update!, phase: "prepared" } }; await mount();
  expect(button(m("pluginPrewarmPolicy.cancel")).disabled).toBe(false);
  expect(() => button(m("pluginPrewarmPolicy.forget"))).toThrow();
  vi.mocked(executePluginPrewarmPolicyCommand).mockImplementationOnce(async (command) => {
    // Cancel retains the original request and leaves the effective empty rules unchanged.
    // 取消保留原请求，保持生效空规则不变。
    const cancelled = { ...published.update!, request: command.request, phase: "cancelled" as const };
    policy = { ...policy, update: cancelled }; return cancelled;
  });
  await act(async () => button(m("pluginPrewarmPolicy.cancel")).click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "cancel", request: published.update!.request });
  expect(policy.rules).toEqual([]);
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.receipt.cancelled"));
  expect(button(m("pluginPrewarmPolicy.forget")).disabled).toBe(false);
});

// A confirmed save stays visible even when the mandatory following full read fails.
// 即使必需的后续完整读取失败，已确认保存仍然可见。
it("shows an acknowledged publication separately from stale status", async () => {
  policy = { ...published, update: null }; await mount();
  await act(async () => button(m("pluginPrewarmPolicy.edit")).click());
  vi.mocked(readPluginPrewarmPolicyStatus).mockRejectedValueOnce(new Error("refresh offline"));
  await act(async () => button(m("pluginPrewarmPolicy.save")).click());
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.receipt.committed"));
  expect(host.textContent).toContain(m("pluginPrewarmPolicy.stale"));
  expect(host.textContent).not.toContain(m("pluginPrewarmPolicy.unknown"));
  expect(button(m("pluginPrewarmPolicy.forget")).disabled).toBe(true);
  expect(button(m("pluginPrewarmPolicy.edit")).disabled).toBe(true);
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
});
