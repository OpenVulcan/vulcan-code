// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-service";
import { PluginUpdateControls } from "./plugin-update-controls";
import { readLocaleMessage } from "@/i18n";

// Select an actual supported locale for DOM checks while keeping each test's language independent.
// 为 DOM 检查选择实际支持语言，同时保持各测试语言独立。
const testLocale = vi.hoisted(() => ({ value: "zh-CN" as "zh-CN" | "en-US" }));

vi.mock("@/i18n", async (importOriginal) => {
  // Keep real translated labels so tests exercise the actual user-facing controls.
  // 保留真实翻译标签，使测试验证实际面向用户的控件。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: testLocale.value, t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage(testLocale.value, key) }) };
});

// The fixed original identity makes accidental replacement requests observable across retries and collapse.
// 固定原身份使跨重试及折叠的意外替代请求可被观察。
const original: LocalPluginUpdateStatus = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "fixture", phase: "published",
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abd", version: "2.0.0", content_fingerprint: "approved-by-service" },
  activationRevision: "4", installation: "succeeded", effects: [], hasServiceOwner: false,
};
// Real DOM and service stubs remain isolated per test; no production update or secure-store write is performed.
// 真实 DOM 及服务替身按测试隔离；不执行生产更新或安全存储写入。
let host: HTMLDivElement;
let root: Root;
const read = vi.fn<(pluginId: string) => Promise<LocalPluginUpdateStatus[]>>();
const resume = vi.fn<(pluginId: string, requestId: string) => Promise<void>>();
const abandon = vi.fn<(pluginId: string, requestId: string, acknowledgement: boolean) => Promise<LocalPluginUpdateStatus>>();

// button locates one control by its visible label; returns its DOM element or a precise stale-fixture error.
// button 按可见标签定位单个控件；返回 DOM 元素或精确过期夹具错误。
function button(label: string): HTMLButtonElement {
  // Portaled confirmation controls are searched semantically, never by structural index.
  // 通过语义搜索门户确认控件，绝不依赖结构下标。
  const match = [...document.querySelectorAll("button")].find((element) => element.textContent === label);
  if (!match) throw new Error(`Expected visible button ${label}; its label or availability may have changed`);
  return match;
}

// enterOwner dispatches a real input event for the explicitly entered plugin identity; returns after React settles.
// enterOwner 为明确输入的插件身份发送真实输入事件；在 React 稳定后返回。
async function enterOwner(value: string): Promise<void> {
  // The panel has one semantic plugin identifier input.
  // 面板具有单个语义插件标识输入框。
  const input = host.querySelector<HTMLInputElement>("input");
  if (!input) throw new Error("Expected plugin identifier input; check the recovery panel structure");
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// openAndRead opens the inert panel and queries a named owner; no update action runs.
// openAndRead 打开静止面板并查询指定所有者；不运行更新操作。
async function openAndRead(): Promise<void> {
  await act(async () => button("插件更新记录").click());
  await enterOwner("fixture");
  await act(async () => button("读取更新记录").click());
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  testLocale.value = "zh-CN";
  read.mockResolvedValue([original]); resume.mockResolvedValue(); abandon.mockResolvedValue({ ...original, phase: "aborted" });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await act(async () => root.render(<PluginUpdateControls plugins={[]} actions={{
    onReadLocalPluginUpdates: read, onResumeLocalPluginUpdate: resume, onAbortLocalPluginUpdate: abandon,
  }} />));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Exact candidate observations are visible without creating any automatic lifecycle or recovery action.
// 精确候选观测可见，不创建任何自动生命周期或恢复动作。
it("shows probe, enable and compensation evidence with distinct observations", async () => {
  read.mockResolvedValue([{ ...original, phase: "prepared", effects: [
    { operation: "drain_predecessor_lua", phase: "succeeded" },
    { operation: "disable_predecessor", phase: "succeeded" },
    { operation: "probe", phase: "succeeded" },
    { operation: "enable", phase: "observation_failed" },
    { operation: "enable_rollback", phase: "succeeded" },
    { operation: "restore_predecessor", phase: "started" },
  ] }]);
  await openAndRead();
  expect(host.textContent).toContain("旧 Lua 实例退役: 已观察到成功");
  expect(host.textContent).toContain("旧插件停用回调: 已观察到成功");
  expect(host.textContent).toContain("候选探测: 已观察到成功");
  expect(host.textContent).toContain("候选启用: 结果不确定，请核对外部状态");
  expect(host.textContent).toContain("启用补偿: 已观察到成功");
  expect(host.textContent).toContain("旧插件恢复回调: 已记录尝试，完成情况待核对");
  expect(resume).not.toHaveBeenCalled();
  expect(abandon).not.toHaveBeenCalled();
});

// Preparing requests permit only explicitly acknowledged abandonment, not committed-route resumption.
// 准备中请求仅允许明确确认的放弃，不允许已提交路由恢复。
it("shows an early request and abandons only its original identity", async () => {
  read.mockResolvedValue([{ ...original, phase: "preparing", installation: null }]);
  await openAndRead();
  expect(host.textContent).toContain("正在准备候选");
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "补齐已提交更新")).toBe(false);
  await act(async () => button("放弃未发布请求").click());
  expect(abandon).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "aborted", installation: null }]);
  await act(async () => button("确认").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
});

// Journal-less and preparing service owners may resume only their already retained original request.
// 无日志及准备中的服务所有者仅可恢复已经保留的原请求。
it.each([null, "preparing"] as const)("resumes an owned early request with phase %s without inventing a journal", async (phase) => {
  read.mockResolvedValue([{ ...original, phase, installation: null, hasServiceOwner: true }]);
  await openAndRead();
  expect(host.textContent).toContain(phase === null ? "准备预留" : "正在准备候选");
  expect(button("继续原请求准备").disabled).toBe(false);
  expect(button("放弃未发布请求").disabled).toBe(false);
  expect(resume).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await act(async () => button("继续原请求准备").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(host.textContent).toContain("已完成");
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "补齐已提交更新")).toBe(false);
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "继续原请求准备")).toBe(false);
  expect(button("完成原请求清理").disabled).toBe(false);
  expect(abandon).not.toHaveBeenCalled();
});

// Null abandonment requires actual service ownership and the existing explicit external-effect acknowledgement.
// 空阶段放弃要求实际服务所有权及既有明确外部副作用确认。
it("abandons an owned journal-less reservation only after explicit acknowledgement", async () => {
  read.mockResolvedValue([{ ...original, phase: null, installation: null, hasServiceOwner: true }]);
  await openAndRead();
  await act(async () => button("放弃未发布请求").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  read.mockResolvedValue([{ ...original, phase: "aborted", installation: null, hasServiceOwner: true }]);
  await act(async () => button("确认").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
});

// Ownership does not authorize prepared replay; terminal records without an owner and committed-route rules stay unchanged.
// 所有权不授权已准备记录重播；无所有者终态记录及已提交路由规则保持不变。
it.each([
  { phase: null, hasServiceOwner: false, canResume: false, canAbort: false },
  { phase: "preparing", hasServiceOwner: false, canResume: false, canAbort: true },
  { phase: "prepared", hasServiceOwner: true, canResume: false, canAbort: true },
  { phase: "committing", hasServiceOwner: false, canResume: true, canAbort: true },
  { phase: "published", hasServiceOwner: false, canResume: true, canAbort: false },
  { phase: "completed", hasServiceOwner: false, canResume: false, canAbort: false },
  { phase: "aborted", hasServiceOwner: false, canResume: false, canAbort: false },
] satisfies { phase: LocalPluginUpdateStatus["phase"]; hasServiceOwner: boolean; canResume: boolean; canAbort: boolean }[])
  ("gates original actions for phase $phase and owner $hasServiceOwner", async ({ phase, hasServiceOwner, canResume, canAbort }) => {
    read.mockResolvedValue([{ ...original, phase, hasServiceOwner, installation: null }]);
    await openAndRead();
    // Observe semantic control availability rather than structural row or child positions.
    // 观察语义控件可用性，不依赖结构行或子项位置。
    const labels = [...document.querySelectorAll("button")].map((element) => element.textContent);
    expect(labels.includes("补齐已提交更新")).toBe(canResume);
    expect(labels.includes("继续原请求准备")).toBe(false);
    expect(labels.includes("放弃未发布请求")).toBe(canAbort);
    expect(resume).not.toHaveBeenCalled();
    expect(abandon).not.toHaveBeenCalled();
  });

// Only a terminal request's actual service owner enables original cleanup, never update preparation or abandonment.
// 仅终态请求的实际服务所有者启用原清理，绝不启用更新准备或再次放弃。
it.each([
  { phase: "completed", hasServiceOwner: true }, { phase: "completed", hasServiceOwner: false },
  { phase: "aborted", hasServiceOwner: true }, { phase: "aborted", hasServiceOwner: false },
] as const)("exposes only cleanup for terminal phase $phase and owner $hasServiceOwner", async ({ phase, hasServiceOwner }) => {
  read.mockResolvedValue([{ ...original, phase, hasServiceOwner }]);
  await openAndRead();
  // Semantic labels separate resource cleanup from every mutation that could suggest replay.
  // 语义标签将资源清理与任何可能暗示重播的变更区分。
  const labels = [...document.querySelectorAll("button")].map((element) => element.textContent);
  expect(labels.includes("完成原请求清理")).toBe(hasServiceOwner);
  expect(labels.includes("补齐已提交更新")).toBe(false);
  expect(labels.includes("继续原请求准备")).toBe(false);
  expect(labels.includes("放弃未发布请求")).toBe(false);
  expect(resume).not.toHaveBeenCalled();
  expect(abandon).not.toHaveBeenCalled();
});

// Completed cleanup reuses the original resume interface and UUID; actual owner release removes its sole action.
// 已完成清理复用原恢复接口及 UUID；实际所有者释放移除其唯一操作。
it("finishes a completed original owner through resume and hides cleanup after owner release", async () => {
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await openAndRead();
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: false }]);
  await act(async () => button("完成原请求清理").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(abandon).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledTimes(2);
  expect(host.textContent).toContain(original.requestId);
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "完成原请求清理")).toBe(false);
});

// Aborted cleanup confirms existing resource retirement without asking to repeat the original external-effect decision.
// 已放弃清理确认既有资源退役，不要求重复原外部副作用决定。
it("finishes an aborted original owner through abort with a cleanup-specific confirmation", async () => {
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: true }]);
  await openAndRead();
  await act(async () => button("完成原请求清理").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("仅完成原请求仍持有资源的清理");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("历史外部副作用不会因此撤销，未知结果仍需核对");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).not.toContain("放弃未发布请求");
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: false }]);
  await act(async () => button("确认").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledTimes(2);
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "完成原请求清理")).toBe(false);
});

// Uncertain terminal cleanup retains the exact row and blocks another attempt until explicit authoritative readback.
// 不确定终态清理保留精确行，并阻止再次尝试，直至明确权威回读。
it.each(["completed", "aborted"] as const)("retains failed %s cleanup without automatic retry", async (phase) => {
  read.mockResolvedValue([{ ...original, phase, hasServiceOwner: true }]);
  await openAndRead();
  if (phase === "completed") resume.mockRejectedValueOnce(new Error("cleanup uncertain"));
  else abandon.mockRejectedValueOnce(new Error("cleanup uncertain"));
  read.mockRejectedValueOnce(new Error("owner readback unavailable"));
  await act(async () => button("完成原请求清理").click());
  if (phase === "aborted") await act(async () => button("确认").click());
  expect(host.textContent).toContain(original.requestId);
  expect(host.textContent).toContain("cleanup uncertain");
  expect(host.textContent).toContain("owner readback unavailable");
  expect(button("完成原请求清理").disabled).toBe(true);
  expect(resume).toHaveBeenCalledTimes(phase === "completed" ? 1 : 0);
  expect(abandon).toHaveBeenCalledTimes(phase === "aborted" ? 1 : 0);
  await act(async () => button("读取更新记录").click());
  expect(button("完成原请求清理").disabled).toBe(false);
  expect(resume).toHaveBeenCalledTimes(phase === "completed" ? 1 : 0);
  expect(abandon).toHaveBeenCalledTimes(phase === "aborted" ? 1 : 0);
  read.mockResolvedValue([{ ...original, phase, hasServiceOwner: false }]);
  await act(async () => button("读取更新记录").click());
  expect([...document.querySelectorAll("button")].some((element) => element.textContent === "完成原请求清理")).toBe(false);
});

// English controls express resource cleanup and retained historical effects rather than restarting update work.
// 英文控件表达资源清理及保留历史副作用，而非重新启动更新工作。
it("renders English original cleanup labels and confirmation without another update operation", async () => {
  testLocale.value = "en-US";
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: true }]);
  await act(async () => root.render(<PluginUpdateControls plugins={[]} actions={{
    onReadLocalPluginUpdates: read, onResumeLocalPluginUpdate: resume, onAbortLocalPluginUpdate: abandon,
  }} />));
  await act(async () => button(readLocaleMessage("en-US", "pluginUpdates.title")).click());
  await enterOwner("fixture");
  await act(async () => button(readLocaleMessage("en-US", "pluginUpdates.read")).click());
  await act(async () => button("Finish original request cleanup").click());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Updates and callbacks are not replayed.");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Historical external effects are not undone; uncertain outcomes still require verification.");
  expect(abandon).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: false }]);
  await act(async () => button(readLocaleMessage("en-US", "skills.plugins.confirm")).click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
});

// Opening is inert; explicit reads preserve the request, while recovery re-reads authoritative completion.
// 展开静止；明确读取保留请求，而恢复重新读取权威完成状态。
it("reads only on explicit request and recovers the original committed identity", async () => {
  expect(read).not.toHaveBeenCalled();
  await openAndRead();
  expect(read).toHaveBeenCalledWith("fixture"); expect(resume).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "completed" }]);
  await act(async () => button("补齐已提交更新").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(read).toHaveBeenCalledTimes(2); expect(host.textContent).toContain("已完成");
  expect(abandon).not.toHaveBeenCalled();
});

// Late observations from another owner cannot enable recovery or replace a new owner selection.
// 来自另一所有者的迟到观测不能启用恢复或覆盖新所有者选择。
it("discards a late response after plugin identity changes", async () => {
  // Retain the actual pending response resolver to control the observation race.
  // 保留实际待完成响应解析器，以控制观测竞争。
  let resolve!: (records: LocalPluginUpdateStatus[]) => void;
  read.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  await openAndRead();
  await enterOwner("another-plugin");
  await act(async () => resolve([original]));
  expect(host.textContent).not.toContain(original.requestId);
  expect(resume).not.toHaveBeenCalled();
  expect(button("读取更新记录").disabled).toBe(false);
});

// A lost mutation response is preserved through collapse and reconciled by reading without automatic replay.
// 丢失的变更响应跨折叠保留，并通过读取核对，不自动重放。
it("retains mutation errors and reads completion without retrying the operation", async () => {
  await openAndRead(); resume.mockRejectedValueOnce(new Error("response lost"));
  read.mockResolvedValue([{ ...original, phase: "completed" }]);
  await act(async () => button("补齐已提交更新").click());
  expect(resume).toHaveBeenCalledTimes(1); expect(host.textContent).toContain("response lost");
  expect(host.textContent).toContain("已完成");
  await act(async () => button("插件更新记录").click());
  await act(async () => button("插件更新记录").click());
  expect(read).toHaveBeenCalledTimes(2); expect(host.textContent).toContain("response lost");
});

// Failed readback blocks further actions until another explicit successful observation.
// 回读失败阻止后续操作，直至另一次明确成功观测。
it("disables actions when readback fails even after a successful mutation response", async () => {
  await openAndRead(); read.mockRejectedValueOnce(new Error("status unavailable"));
  await act(async () => button("补齐已提交更新").click());
  expect(button("补齐已提交更新").disabled).toBe(true);
  expect(host.textContent).toContain("status unavailable");
  await act(async () => button("读取更新记录").click());
  expect(button("补齐已提交更新").disabled).toBe(false);
});

// Abandonment requires shared confirmation that names the original request and preserves external effects.
// 放弃要求共享确认明确原请求并保留外部副作用。
it("requires explicit effect acknowledgement before abandoning an unpublished request", async () => {
  read.mockResolvedValue([{ ...original, phase: "prepared", installation: "started" }]);
  await openAndRead();
  await act(async () => button("放弃未发布请求").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("不会撤销");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  read.mockResolvedValue([{ ...original, phase: "aborted" }]);
  await act(async () => button("确认").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(host.textContent).toContain("已放弃");
  expect(resume).not.toHaveBeenCalled();
});

// Repeated clicks share one synchronous admission; disappearing original records retain stale recovery evidence.
// 重复点击共享单次同步入场；原记录消失时保留过期恢复证据。
it("blocks repeat clicks and retains the original record when readback loses it", async () => {
  await openAndRead(); read.mockResolvedValue([]);
  await act(async () => { button("补齐已提交更新").click(); button("补齐已提交更新").click(); });
  expect(resume).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(original.requestId);
  expect(host.textContent).toContain("原更新记录缺失");
  expect(button("补齐已提交更新").disabled).toBe(true);
  await act(async () => button("读取更新记录").click());
  expect(host.textContent).toContain(original.requestId);
  expect(button("补齐已提交更新").disabled).toBe(true);
  await enterOwner("another-plugin");
  read.mockResolvedValue([{ ...original, pluginId: "another-plugin", requestId: original.candidate.generation_id,
    candidate: { ...original.candidate, plugin_id: "another-plugin" } }]);
  await act(async () => button("读取更新记录").click());
  expect(host.textContent).toContain(original.requestId);
  expect(button("补齐已提交更新").disabled).toBe(true);
  await enterOwner("fixture"); read.mockResolvedValue([{ ...original, phase: "completed" }]);
  await act(async () => button("读取更新记录").click());
  expect(host.textContent).toContain("已完成");
  expect(host.textContent).not.toContain("原操作结果尚未确认");
});
