// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import wire from "@/service/__fixtures__/plugin-prewarm-policy-wire.json";
import { parsePluginPrewarmPolicyStatus } from "@/service/plugin-prewarm-policy-contract";
import { executePluginPrewarmPolicyCommand, readPluginPrewarmPolicyStatus } from "@/service/plugin-prewarm-policy-service";
import type { PluginPrewarmPolicyStatus } from "@/types/plugin-prewarm-policy";
import { PluginPrewarmPolicyControls } from "./plugin-prewarm-policy-controls";

vi.mock("@/service/plugin-prewarm-policy-service", () => ({ executePluginPrewarmPolicyCommand: vi.fn(), readPluginPrewarmPolicyStatus: vi.fn() }));
vi.mock("@/i18n", async (importOriginal) => {
  // actual retains production localization while selecting a deterministic UI language.
  // actual 保留生产本地化，同时选择确定的界面语言。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: "zh-CN", t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key) }) };
});

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
  const match = [...scope.querySelectorAll("button")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected button ${label}; check current UI labels`);
  return match;
}

// fill changes the explicitly labelled controlled input inside scope through a real browser event.
// fill 通过真实浏览器事件修改 scope 内具有明确标签的受控输入。
async function fill(label: string, value: string, scope: ParentNode = host): Promise<void> {
  // associated resolves the input from the label relationship instead of assuming field ordering.
  // associated 根据标签关联解析输入，不假定字段顺序。
  const associated = [...scope.querySelectorAll("label")].find((element) => element.firstChild?.textContent === label);
  if (!associated) throw new Error(`Expected field ${label}; check current UI labels`);
  const input = document.getElementById(associated.htmlFor) as HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}

// openAndEdit explicitly opens, reads and creates a draft; it performs no policy write.
// openAndEdit 明确展开、读取并创建草稿；不写入策略。
async function openAndEdit(): Promise<void> {
  await act(async () => button("自动预热策略").click());
  await act(async () => button("读取策略与发布回执").click());
  await act(async () => button("从当前规则开始编辑").click());
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
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
  await act(async () => root.render(<PluginPrewarmPolicyControls />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Opening is inert; the real editor saves exact fields once and exposes the original terminal receipt.
// 展开不产生操作；真实编辑器只保存一次精确字段，并展示原终态回执。
it("explicitly edits and saves complete rules without initializing plugins", async () => {
  expect(readPluginPrewarmPolicyStatus).not.toHaveBeenCalled();
  await openAndEdit();
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
  await act(async () => button("添加规则").click());
  expect(button("保存完整规则").disabled).toBe(true);
  await fill("插件标识", "control-plugin"); await fill("工具标识", "control_tool"); await fill("目标空闲 VM 数", "2");
  await act(async () => { button("保存完整规则").click(); button("保存完整规则").click(); });
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "begin", request: {
    updateId: expect.any(String), runtimeId: wire.initial.runtimeId, expectedRevision: wire.initial.revision,
    rules: [{ pluginId: "control-plugin", toolId: "control_tool", targetIdle: 2 }],
  } });
  expect(host.textContent).toContain("原策略发布事务");
  expect(button("取消未发布准备").disabled).toBe(true);
  await act(async () => button("确认终态发布回执").click());
  expect(policy.update).toBeNull();
  expect(policy.rules).toEqual([{ pluginId: "control-plugin", toolId: "control_tool", targetIdle: 2 }]);
});

// Removing the last rule must submit an explicit empty collection rather than omitting the setting.
// 移除最后规则必须提交明确空集合，不能省略设置。
it("disables automatic policy by saving an explicit empty replacement", async () => {
  policy = { ...published, update: null };
  await openAndEdit();
  await act(async () => button("移除此规则").click());
  expect(host.textContent).toContain("保存空集合将关闭自动预热");
  await act(async () => button("保存完整规则").click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "begin", request: {
    updateId: expect.any(String), runtimeId: published.runtimeId, expectedRevision: published.revision, rules: [],
  } });
});

// Invalid counts and duplicate globally unique tool identities cannot reach the service.
// 无效数量及重复全局唯一工具身份不能到达服务。
it("rejects invalid counts and duplicate tool identities in the actual form", async () => {
  policy = { ...published, update: null }; await openAndEdit();
  for (const invalid of ["", "0", "-1", "1.5", "01", "9007199254740992"]) {
    await fill("目标空闲 VM 数", invalid);
    expect(button("保存完整规则").disabled).toBe(true);
  }
  await fill("目标空闲 VM 数", "1");
  await act(async () => button("添加规则").click());
  // The blank plugin input identifies the newly added row without relying on its array position.
  // 空插件输入标识新增行，不依赖数组位置。
  const empty = [...host.querySelectorAll("input")].find((input) => input.id.endsWith("-plugin") && input.value === "");
  if (!empty?.parentElement?.parentElement) throw new Error("Expected a blank new rule row; check editor semantics");
  const row = empty.parentElement.parentElement;
  await fill("插件标识", "other-plugin", row); await fill("工具标识", "control_tool", row); await fill("目标空闲 VM 数", "1", row);
  expect(button("保存完整规则").disabled).toBe(true);
  expect(executePluginPrewarmPolicyCommand).not.toHaveBeenCalled();
});

// Reads preserve edits but cannot silently rebase them onto a replacement parent or new revision.
// 读取保留编辑，但不能静默将其重定位到替换父级或新修订。
it("preserves drafts through failed reads and rejects changed predecessors", async () => {
  policy = { ...published, update: null }; await openAndEdit();
  await fill("目标空闲 VM 数", "2");
  vi.mocked(readPluginPrewarmPolicyStatus).mockRejectedValueOnce(new Error("offline"));
  await act(async () => button("读取策略与发布回执").click());
  expect(host.querySelector("fieldset")?.disabled).toBe(true);
  expect([...host.querySelectorAll("input")].some((input) => input.value === "2")).toBe(true);
  policy = { ...policy, runtimeId: "replacement", revision: "new-policy" };
  await act(async () => button("读取策略与发布回执").click());
  expect(host.textContent).toContain("运行时或策略已变化");
  expect(host.querySelector("fieldset")?.disabled).toBe(true);
  await act(async () => button("从当前规则开始编辑").click());
  expect(host.querySelector("fieldset")?.disabled).toBe(false);
  expect([...host.querySelectorAll("input")].some((input) => input.value === "1")).toBe(true);
});

// Collapse cannot discard an unknown write; explicit observation and acknowledgement never resend it.
// 折叠不能丢弃未知写入；明确观测及确认绝不重发。
it("retains unknown publication across collapse until explicitly inspected", async () => {
  await openAndEdit();
  vi.mocked(executePluginPrewarmPolicyCommand).mockRejectedValueOnce(new Error("response lost"));
  await act(async () => button("保存完整规则").click());
  expect(host.textContent).toContain("发布结果未知");
  expect(button("已核查未知结果").disabled).toBe(true);
  await act(async () => button("自动预热策略").click());
  await act(async () => button("自动预热策略").click());
  expect(host.textContent).toContain("发布结果未知");
  await act(async () => button("读取策略与发布回执").click());
  expect(host.querySelector("fieldset")?.disabled).toBe(true);
  await act(async () => button("已核查未知结果").click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
  expect(host.querySelector("fieldset")?.disabled).toBe(false);
});

// Capacity retention and an uncertain final publication expose distinct restrictions in actual controls.
// 容量保留及未确认最终发布在真实控件中暴露不同限制。
it("shows capacity exclusion and protects publishing from cancellation or forgetting", async () => {
  policy = { ...policy, capacityUpdateRetained: true }; await openAndEdit();
  expect(button("从当前规则开始编辑").disabled).toBe(true);
  expect(host.textContent).toContain("容量事务仍保留");
  policy = { ...published, update: { ...published.update!, phase: "publishing", persistenceError: "write confirmation lost" } };
  await act(async () => button("读取策略与发布回执").click());
  expect(button("恢复原发布").disabled).toBe(false);
  expect(button("取消未发布准备").disabled).toBe(true);
  expect(button("确认终态发布回执").disabled).toBe(true);
  await act(async () => button("恢复原发布").click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "resume", request: published.update!.request });
});

// A prepared original transaction may be cancelled explicitly without manufacturing a new publication request.
// 已准备原事务可明确取消，不制造新的发布请求。
it("cancels the exact prepared receipt", async () => {
  policy = { ...policy, update: { ...published.update!, phase: "prepared" } };
  await act(async () => button("自动预热策略").click());
  await act(async () => button("读取策略与发布回执").click());
  expect(button("取消未发布准备").disabled).toBe(false);
  expect(button("确认终态发布回执").disabled).toBe(true);
  vi.mocked(executePluginPrewarmPolicyCommand).mockImplementationOnce(async (command) => {
    // Cancel retains the original request and leaves the effective empty rules unchanged.
    // 取消保留原请求，保持生效空规则不变。
    const cancelled = { ...published.update!, request: command.request, phase: "cancelled" as const };
    policy = { ...policy, update: cancelled }; return cancelled;
  });
  await act(async () => button("取消未发布准备").click());
  expect(executePluginPrewarmPolicyCommand).toHaveBeenLastCalledWith({ kind: "cancel", request: published.update!.request });
  expect(policy.rules).toEqual([]);
  expect(button("确认终态发布回执").disabled).toBe(false);
});

// A confirmed save remains inspectable even when the mandatory following full read fails.
// 即使必需的后续完整读取失败，已确认保存仍可核查。
it("shows acknowledged publication separately from stale status", async () => {
  await openAndEdit();
  vi.mocked(readPluginPrewarmPolicyStatus).mockRejectedValueOnce(new Error("refresh offline"));
  await act(async () => button("保存完整规则").click());
  expect(host.textContent).toContain("已确认回执（后续读取失败）");
  expect(host.textContent).not.toContain("发布结果未知");
  expect(host.querySelector("fieldset")?.disabled).toBe(true);
  expect(executePluginPrewarmPolicyCommand).toHaveBeenCalledTimes(1);
});
