// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-service";
import { PluginUpdateControls } from "./plugin-update-controls";
import { formatMessage, readLocaleMessage, type TranslationKey } from "@/i18n";
import type { RecoveryReport } from "./plugin-maintenance-section";

// Select an actual supported locale for DOM checks while keeping each test's language independent.
// 为 DOM 检查选择实际支持语言，同时保持各测试语言独立。
const testLocale = vi.hoisted(() => ({ value: "zh-CN" as "zh-CN" | "en-US" }));

vi.mock("@/i18n", async (importOriginal) => {
  // Keep real translated labels so tests exercise the actual user-facing controls.
  // 保留真实翻译标签，使测试验证实际面向用户的控件。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ locale: testLocale.value, t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage(testLocale.value, key) }) };
});

// The fixed original identity makes accidental replacement requests observable across retries and re-checks.
// 固定原身份使跨重试及重新检查的意外替代请求可被观察。
const original: LocalPluginUpdateStatus = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "fixture", phase: "published",
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abd", version: "2.0.0", content_fingerprint: "approved-by-service" },
  activationRevision: "4", installation: "succeeded", effects: [], hasServiceOwner: false,
};
// Real DOM and service stubs remain isolated per test; no production update or secure-store write is performed.
// 真实 DOM 及服务替身按测试隔离；不执行生产更新或安全存储写入。
let host: HTMLDivElement;
let root: Root;
let scanToken = 0;
const read = vi.fn<(pluginId: string) => Promise<LocalPluginUpdateStatus[]>>();
const resume = vi.fn<(pluginId: string, requestId: string) => Promise<void>>();
const abandon = vi.fn<(pluginId: string, requestId: string, acknowledgement: boolean) => Promise<LocalPluginUpdateStatus>>();
const report = vi.fn<(next: RecoveryReport) => void>();

// m resolves the current test locale's actual message, filling the fixture's name and version.
// m 解析当前测试语言的实际文案，并填入夹具名称与版本。
function m(key: TranslationKey): string {
  return formatMessage(readLocaleMessage(testLocale.value, key), { name: "Fixture", version: original.candidate.version });
}

// labels lists every visible button label, independently from row or child positions.
// labels 列出所有可见按钮标签，不依赖行或子项位置。
function labels(): string[] { return [...document.querySelectorAll("button")].map((element) => element.textContent ?? ""); }

// button locates one control by its visible label; returns its DOM element or a precise stale-fixture error.
// button 按可见标签定位单个控件；返回 DOM 元素或精确过期夹具错误。
function button(key: TranslationKey): HTMLButtonElement {
  // Portaled confirmation controls are searched semantically, never by structural index.
  // 通过语义搜索门户确认控件，绝不依赖结构下标。
  const match = [...document.querySelectorAll("button")].find((element) => element.textContent === m(key));
  if (!match) throw new Error(`Expected visible button ${m(key)}; its label or availability may have changed`);
  return match;
}

// details maps each technical-details term to its value inside root, so evidence is checked by meaning.
// details 将 root 内每个技术详情术语映射到其值，使证据按含义校验。
function details(scope: ParentNode = host): Map<string, string> {
  return new Map([...scope.querySelectorAll(".page-details .page-facts > div")]
    .map((row) => [row.querySelector("dt")?.textContent ?? "", row.querySelector("dd")?.textContent ?? ""]));
}

// dialog returns the open confirmation, which is portaled outside host.
// dialog 返回已打开的确认框，其被门户渲染到 host 之外。
function dialog(): HTMLElement {
  const found = document.querySelector<HTMLElement>('[role="alertdialog"]');
  if (!found) throw new Error("Expected an open confirmation; its trigger may have changed");
  return found;
}

// render mounts one reader for the fixture owner; mounting performs the first read.
// render 为夹具所有者挂载单个读取器；挂载即执行首次读取。
async function render(): Promise<void> {
  await act(async () => root.render(<PluginUpdateControls pluginId="fixture" pluginName="Fixture" scanToken={scanToken} onReport={report} actions={{
    onReadLocalPluginUpdates: read, onResumeLocalPluginUpdate: resume, onAbortLocalPluginUpdate: abandon,
  }} />));
}

// recheck asks the reader to check again, the way the recovery center's re-check button does.
// recheck 以恢复中心重新检查按钮的方式要求读取器再次检查。
async function recheck(): Promise<void> { scanToken += 1; await render(); }

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  testLocale.value = "zh-CN"; scanToken = 0;
  read.mockResolvedValue([original]); resume.mockResolvedValue(); abandon.mockResolvedValue({ ...original, phase: "aborted" });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
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
  await render();
  const evidence = details();
  expect(evidence.get(m("pluginUpdates.drainPredecessorLua"))).toBe(m("pluginUpdates.installSucceeded"));
  expect(evidence.get(m("pluginUpdates.disablePredecessor"))).toBe(m("pluginUpdates.installSucceeded"));
  expect(evidence.get(m("pluginUpdates.probe"))).toBe(m("pluginUpdates.installSucceeded"));
  expect(evidence.get(m("pluginUpdates.enable"))).toBe(m("pluginUpdates.installUncertain"));
  expect(evidence.get(m("pluginUpdates.enableRollback"))).toBe(m("pluginUpdates.installSucceeded"));
  expect(evidence.get(m("pluginUpdates.restorePredecessor"))).toBe(m("pluginUpdates.effectStarted"));
  expect(evidence.get(m("pluginUpdates.request"))).toBe(original.requestId);
  expect(host.textContent).toContain(m("pluginUpdates.itemTitle"));
  expect(resume).not.toHaveBeenCalled();
  expect(abandon).not.toHaveBeenCalled();
});

// Finished history without remaining work needs no decision and reports an empty reader.
// 已结束且无剩余工作的历史无需决定，并报告空读取器。
it("hides finished history and reports nothing to show", async () => {
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: false }]);
  await render();
  expect(host.querySelector("article")).toBeNull();
  expect(report).toHaveBeenLastCalledWith({ loading: false, visible: 0 });
});

// Preparing requests permit only explicitly acknowledged abandonment, not committed-route resumption.
// 准备中请求仅允许明确确认的放弃，不允许已提交路由恢复。
it("shows an early request and abandons only its original identity", async () => {
  read.mockResolvedValue([{ ...original, phase: "preparing", installation: null }]);
  await render();
  expect(host.textContent).toContain(m("pluginUpdates.preparing"));
  expect(labels()).not.toContain(m("pluginUpdates.resume"));
  await act(async () => button("pluginUpdates.abandon").click());
  expect(abandon).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "aborted", installation: null }]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
  expect(host.textContent).toContain(m("pluginUpdates.finishedAbort"));
});

// Journal-less and preparing service owners may resume only their already retained original request.
// 无日志及准备中的服务所有者仅可恢复已经保留的原请求。
it.each([null, "preparing"] as const)("resumes an owned early request with phase %s without inventing a journal", async (phase) => {
  read.mockResolvedValue([{ ...original, phase, installation: null, hasServiceOwner: true }]);
  await render();
  expect(host.textContent).toContain(m(phase === null ? "pluginUpdates.reservingPreparation" : "pluginUpdates.preparing"));
  expect(button("pluginUpdates.resumePreparation").disabled).toBe(false);
  expect(button("pluginUpdates.abandon").disabled).toBe(false);
  expect(resume).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await act(async () => button("pluginUpdates.resumePreparation").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(host.textContent).toContain(m("pluginUpdates.completed"));
  expect(labels()).not.toContain(m("pluginUpdates.resume"));
  expect(labels()).not.toContain(m("pluginUpdates.resumePreparation"));
  expect(button("pluginUpdates.finishCleanup").disabled).toBe(false);
  expect(abandon).not.toHaveBeenCalled();
});

// Null abandonment requires actual service ownership and the existing explicit external-effect acknowledgement.
// 空阶段放弃要求实际服务所有权及既有明确外部副作用确认。
it("abandons an owned journal-less reservation only after explicit confirmation", async () => {
  read.mockResolvedValue([{ ...original, phase: null, installation: null, hasServiceOwner: true }]);
  await render();
  await act(async () => button("pluginUpdates.abandon").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(details(dialog()).get(m("pluginUpdates.request"))).toBe(original.requestId);
  read.mockResolvedValue([{ ...original, phase: "aborted", installation: null, hasServiceOwner: true }]);
  await act(async () => button("skills.plugins.confirm").click());
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
    await render();
    expect(labels().includes(m("pluginUpdates.resume"))).toBe(canResume);
    expect(labels().includes(m("pluginUpdates.resumePreparation"))).toBe(false);
    expect(labels().includes(m("pluginUpdates.abandon"))).toBe(canAbort);
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
  await render();
  expect(labels().includes(m("pluginUpdates.finishCleanup"))).toBe(hasServiceOwner);
  expect(labels().includes(m("pluginUpdates.resume"))).toBe(false);
  expect(labels().includes(m("pluginUpdates.resumePreparation"))).toBe(false);
  expect(labels().includes(m("pluginUpdates.abandon"))).toBe(false);
  expect(resume).not.toHaveBeenCalled();
  expect(abandon).not.toHaveBeenCalled();
});

// Completed cleanup reuses the original resume interface and UUID; actual owner release removes its sole action.
// 已完成清理复用原恢复接口及 UUID；实际所有者释放移除其唯一操作。
it("finishes a completed original owner through resume and hides cleanup after owner release", async () => {
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await render();
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: false }]);
  await act(async () => button("pluginUpdates.finishCleanup").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(abandon).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledTimes(2);
  expect(host.textContent).toContain(m("pluginUpdates.finishedResume"));
  expect(labels()).not.toContain(m("pluginUpdates.finishCleanup"));
});

// Aborted cleanup confirms existing resource retirement without asking to repeat the original external-effect decision.
// 已放弃清理确认既有资源退役，不要求重复原外部副作用决定。
it("finishes an aborted original owner through abort with a cleanup-specific confirmation", async () => {
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: true }]);
  await render();
  await act(async () => button("pluginUpdates.finishCleanup").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(dialog().textContent).toContain(m("pluginUpdates.cleanupWarning"));
  expect(dialog().textContent).not.toContain(m("pluginUpdates.abandonWarning"));
  expect(details(dialog()).get(m("pluginUpdates.request"))).toBe(original.requestId);
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: false }]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
  expect(read).toHaveBeenCalledTimes(2);
  expect(labels()).not.toContain(m("pluginUpdates.finishCleanup"));
});

// Uncertain terminal cleanup retains the exact row and blocks another attempt until an authoritative re-check.
// 不确定终态清理保留精确行，并阻止再次尝试，直至权威重新检查。
it.each(["completed", "aborted"] as const)("retains failed %s cleanup without automatic retry", async (phase) => {
  read.mockResolvedValue([{ ...original, phase, hasServiceOwner: true }]);
  await render();
  if (phase === "completed") resume.mockRejectedValueOnce(new Error("cleanup uncertain"));
  else abandon.mockRejectedValueOnce(new Error("cleanup uncertain"));
  read.mockRejectedValueOnce(new Error("owner readback unavailable"));
  await act(async () => button("pluginUpdates.finishCleanup").click());
  if (phase === "aborted") await act(async () => button("skills.plugins.confirm").click());
  expect(details().get(m("pluginUpdates.request"))).toBe(original.requestId);
  expect(host.textContent).toContain("cleanup uncertain");
  expect(host.textContent).toContain("owner readback unavailable");
  expect(host.textContent).toContain(m("pluginUpdates.unconfirmed"));
  expect(button("pluginUpdates.finishCleanup").disabled).toBe(true);
  expect(resume).toHaveBeenCalledTimes(phase === "completed" ? 1 : 0);
  expect(abandon).toHaveBeenCalledTimes(phase === "aborted" ? 1 : 0);
  await recheck();
  expect(button("pluginUpdates.finishCleanup").disabled).toBe(false);
  expect(host.textContent).not.toContain(m("pluginUpdates.unconfirmed"));
  expect(resume).toHaveBeenCalledTimes(phase === "completed" ? 1 : 0);
  expect(abandon).toHaveBeenCalledTimes(phase === "aborted" ? 1 : 0);
  read.mockResolvedValue([{ ...original, phase, hasServiceOwner: false }]);
  await recheck();
  expect(labels()).not.toContain(m("pluginUpdates.finishCleanup"));
});

// English controls express resource cleanup and retained historical effects rather than restarting update work.
// 英文控件表达资源清理及保留历史副作用，而非重新启动更新工作。
it("renders English cleanup labels and confirmation without another update operation", async () => {
  testLocale.value = "en-US";
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: true }]);
  await render();
  await act(async () => button("pluginUpdates.finishCleanup").click());
  expect(dialog().textContent).toContain(m("pluginUpdates.cleanupWarning"));
  expect(abandon).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "aborted", hasServiceOwner: false }]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(resume).not.toHaveBeenCalled();
});

// Mounting reads once without acting; recovery re-reads authoritative completion of the original committed identity.
// 挂载仅读取一次而不执行操作；恢复重新读取原已提交身份的权威完成状态。
it("reads on mount and recovers the original committed identity", async () => {
  await render();
  expect(read).toHaveBeenCalledExactlyOnceWith("fixture"); expect(resume).not.toHaveBeenCalled();
  read.mockResolvedValue([{ ...original, phase: "completed" }]);
  await act(async () => button("pluginUpdates.resume").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId);
  expect(read).toHaveBeenCalledTimes(2); expect(host.textContent).toContain(m("pluginUpdates.finishedResume"));
  expect(abandon).not.toHaveBeenCalled();
});

// A reply that arrives after the reader is gone cannot enable recovery.
// 读取器消失后到达的回复不能启用恢复。
it("discards a late response after disposal", async () => {
  // Retain the actual pending response resolver to control the observation race.
  // 保留实际待完成响应解析器，以控制观测竞争。
  let resolve!: (records: LocalPluginUpdateStatus[]) => void;
  read.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  await render();
  await act(async () => root.render(null));
  await act(async () => resolve([original]));
  expect(host.querySelector("article")).toBeNull();
  expect(resume).not.toHaveBeenCalled();
});

// A lost mutation response stays visible and is reconciled by reading, never by an automatic replay.
// 丢失的变更响应保持可见，并通过读取核对，绝不自动重放。
it("retains mutation errors and reads completion without retrying the operation", async () => {
  await render(); resume.mockRejectedValueOnce(new Error("response lost"));
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await act(async () => button("pluginUpdates.resume").click());
  expect(resume).toHaveBeenCalledTimes(1); expect(host.textContent).toContain("response lost");
  expect(host.textContent).toContain(m("pluginUpdates.completed"));
  expect(host.textContent).not.toContain(m("pluginUpdates.finishedResume"));
  await recheck();
  expect(read).toHaveBeenCalledTimes(3); expect(resume).toHaveBeenCalledTimes(1);
});

// Failed readback blocks further actions until another successful observation.
// 回读失败阻止后续操作，直至另一次成功观测。
it("disables actions when readback fails even after a successful mutation response", async () => {
  await render(); read.mockRejectedValueOnce(new Error("status unavailable"));
  await act(async () => button("pluginUpdates.resume").click());
  expect(button("pluginUpdates.resume").disabled).toBe(true);
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("status unavailable");
  await recheck();
  expect(button("pluginUpdates.resume").disabled).toBe(false);
});

// Abandonment requires shared confirmation that names the original request and preserves external effects.
// 放弃要求共享确认明确原请求并保留外部副作用。
it("requires explicit confirmation before abandoning an unpublished request", async () => {
  read.mockResolvedValue([{ ...original, phase: "prepared", installation: "started" }]);
  await render();
  await act(async () => button("pluginUpdates.abandon").click());
  expect(abandon).not.toHaveBeenCalled();
  expect(dialog().textContent).toContain(m("pluginUpdates.abandonWarning"));
  expect(details(dialog()).get(m("pluginUpdates.request"))).toBe(original.requestId);
  read.mockResolvedValue([{ ...original, phase: "aborted" }]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(abandon).toHaveBeenCalledExactlyOnceWith("fixture", original.requestId, true);
  expect(host.textContent).toContain(m("pluginUpdates.finishedAbort"));
  expect(resume).not.toHaveBeenCalled();
});

// Repeated clicks share one synchronous admission; a disappearing original keeps the outcome unconfirmed.
// 重复点击共享单次同步入场；原记录消失时结果保持未确认。
it("blocks repeat clicks and stays unconfirmed when readback loses the original", async () => {
  await render(); read.mockResolvedValue([]);
  await act(async () => { button("pluginUpdates.resume").click(); button("pluginUpdates.resume").click(); });
  expect(resume).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(m("pluginUpdates.missingOriginal"));
  expect(host.textContent).toContain(m("pluginUpdates.unconfirmed"));
  expect(button("pluginUpdates.resume").disabled).toBe(true);
  await recheck();
  expect(host.textContent).toContain(m("pluginUpdates.unconfirmed"));
  expect(button("pluginUpdates.resume").disabled).toBe(true);
  read.mockResolvedValue([{ ...original, phase: "completed", hasServiceOwner: true }]);
  await recheck();
  expect(host.textContent).toContain(m("pluginUpdates.completed"));
  expect(host.textContent).not.toContain(m("pluginUpdates.unconfirmed"));
  expect(resume).toHaveBeenCalledTimes(1);
});
