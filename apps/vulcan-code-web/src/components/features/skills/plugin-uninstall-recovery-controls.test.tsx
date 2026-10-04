// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readLocaleMessage, type TranslationKey } from "@/i18n";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-recovery-service";
import { PluginUninstallRecoveryControls, type PluginUninstallRecoveryProps } from "./plugin-uninstall-recovery-controls";

vi.mock("@/i18n", async (importOriginal) => {
  // actual preserves authoritative translated button labels.
  // actual 保留权威翻译按钮标签。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ t: (key: TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});
// original models an actual retained deletion after its plugin row has disappeared.
// original 模拟插件行消失后的实际保留删除。
const original: PluginUninstallStatus = { requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "hidden-plugin",
  phase: "collection", requiresExternalEffectsAcknowledgement: false };
// read observes all retained originals without requiring any catalog state.
// read 观测所有保留原实例，不要求任何目录状态。
const read = vi.fn<() => Promise<PluginUninstallStatus[]>>();
// resume records only the explicitly reviewed UUID and acknowledgement.
// resume 仅记录明确审阅 UUID 及确认。
const resume = vi.fn<(pluginId: string, requestId: string, acknowledge: boolean) => Promise<void>>();
// acquire models the same synchronous page gate shared with ordinary plugin controls.
// acquire 模拟与普通插件控件共享的相同同步页面门。
const acquire = vi.fn<() => boolean>();
// release records page admission release without cancelling service-owned resources.
// release 记录页面入场释放，不取消服务持有资源。
const release = vi.fn<() => void>();
// confirmed receives only actual successful continuation acknowledgement.
// confirmed 仅接收实际成功继续确认。
const confirmed = vi.fn<(pluginId: string) => void>();
// host owns the real DOM fixture, including actual confirmation portals.
// host 持有真实 DOM 夹具，包括实际确认门户。
let host: HTMLDivElement;
// root owns React rendering and cleanup.
// root 持有 React 渲染及清理。
let root: Root;

// label resolves the actual English translation for key; returns no synthetic copy.
// label 解析 key 的实际英文翻译；不返回合成文案。
function label(key: TranslationKey): string { return readLocaleMessage("en-US", key); }
// button finds key by semantic label; throws if the current fixture contract has changed.
// button 按语义标签查找 key；当前夹具契约变化时抛错。
function button(key: TranslationKey): HTMLButtonElement {
  // found is independent from changing DOM order and row indexes.
  // found 独立于变化 DOM 顺序及行下标。
  const found = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent === label(key));
  if (!found) throw new Error(`Expected ${label(key)}; its label or availability may have changed`);
  return found;
}
// render supplies the declared protocol and optional foreground uncertainty; returns after the mount read.
// render 提供声明协议及可选前台不确定状态；挂载读取后返回。
async function render(overrides: Partial<PluginUninstallRecoveryProps> = {}, strict = false): Promise<void> {
  // controls use the same service mocks for mount, readback and explicit continuation.
  // controls 对挂载、回读及明确继续使用相同服务模拟。
  const controls = <PluginUninstallRecoveryControls actions={{ onReadPluginUninstallRequests: read, onResumePluginUninstallRequest: resume }}
    initialUncertainty={null} onConfirmed={confirmed} mutationBusy={false} acquireMutation={acquire} releaseMutation={release} {...overrides} />;
  await act(async () => root.render(strict ? <React.StrictMode>{controls}</React.StrictMode> : controls));
}
// openAndConfirm reviews the actual original in its confirmation portal without dispatching continuation.
// openAndConfirm 在确认门户审阅实际原实例，不派发继续。
async function openAndConfirm(): Promise<void> {
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  await act(async () => button("pluginRecovery.uninstallContinue").click());
}
beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  read.mockResolvedValue([original]); resume.mockResolvedValue(); acquire.mockReturnValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host); await render();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// A full page mount discovers a hidden plugin read-only; RPC success is authoritative even when its record disappears.
// 整页挂载只读发现隐藏插件；即使记录消失，RPC 成功仍具权威。
it("discovers hidden originals and confirms only a successful exact continuation", async () => {
  expect(read).toHaveBeenCalledExactlyOnceWith(); expect(resume).not.toHaveBeenCalled();
  expect(host.querySelector('input[list]')).toBeNull();
  await openAndConfirm(); expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  expect(document.querySelector('[role="alertdialog"] [role="checkbox"]')).toBeNull();
  read.mockResolvedValue([]); await act(async () => button("skills.plugins.confirm").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith(original.pluginId, original.requestId, false);
  expect(confirmed).toHaveBeenCalledExactlyOnceWith(original.pluginId); expect(release).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(label("pluginRecovery.uninstallConfirmed"));
  expect(host.textContent).not.toContain(label("pluginRecovery.uninstallUnknown"));
});

// Only the explicit service flag exposes acknowledgement, which defaults false until checked.
// 仅明确服务标志暴露确认，勾选前默认为假。
it("shows acknowledgement only when required and sends true only after an explicit check", async () => {
  read.mockResolvedValue([{ ...original, requiresExternalEffectsAcknowledgement: true }]);
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  await act(async () => button("pluginRecovery.read").click());
  await act(async () => button("pluginRecovery.uninstallContinue").click());
  // checkbox is the actual accessible acknowledgement inside the frozen original confirmation.
  // checkbox 是固定原实例确认中的实际无障碍确认框。
  const checkbox = document.querySelector<HTMLButtonElement>('[role="alertdialog"] [role="checkbox"]');
  if (!checkbox) throw new Error("Expected acknowledgement; the service requirement may have changed");
  expect(checkbox.getAttribute("aria-checked")).toBe("false");
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await act(async () => button("skills.plugins.confirm").click()); expect(resume).not.toHaveBeenCalled();
  await act(async () => checkbox.click()); read.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(resume).toHaveBeenCalledExactlyOnceWith(original.pluginId, original.requestId, true);
});

// Failed responses and a missing UUID retain uncertainty through collapse and repeated empty observations.
// 失败响应及缺失 UUID 跨折叠及重复空观测保留不确定状态。
it("keeps a missing original unknown without replay and permits an explicit retry only after rediscovery", async () => {
  await openAndConfirm(); resume.mockRejectedValueOnce(new Error("response lost")); read.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain(original.requestId); expect(host.textContent).toContain(label("pluginRecovery.uninstallUnknown"));
  expect(confirmed).not.toHaveBeenCalled();
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  await act(async () => button("pluginRecovery.read").click()); expect(resume).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(label("pluginRecovery.uninstallUnknown"));
  read.mockResolvedValue([original]); await act(async () => button("pluginRecovery.read").click());
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(false); expect(resume).toHaveBeenCalledTimes(1);
});

// A foreground response without a captured UUID remains explicitly unknown when the service has no matching owner.
// 未捕获 UUID 的前台响应在服务没有匹配所有者时仍明确未知。
it("retains the initial no-ID uncertainty across an empty registry", async () => {
  await render({ initialUncertainty: { pluginId: original.pluginId, requestId: null } }); read.mockResolvedValue([]);
  await act(async () => button("pluginRecovery.read").click());
  expect(host.textContent).toContain(label("pluginRecovery.uninstallUnknown")); expect(host.textContent).toContain(original.pluginId);
  expect(resume).not.toHaveBeenCalled(); expect(confirmed).not.toHaveBeenCalled();
});

// Executing originals and competing ordinary mutations cannot acquire continuation admission.
// 执行中原实例及竞争普通变更不能取得继续入场。
it("respects executing observations and the shared page gate", async () => {
  await openAndConfirm(); acquire.mockReturnValue(false); await act(async () => button("skills.plugins.confirm").click());
  expect(resume).not.toHaveBeenCalled(); expect(release).not.toHaveBeenCalled();
  await act(async () => root.render(null)); read.mockResolvedValue([{ ...original, phase: "executing" }]); await render();
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(true);
});

// Real RPC success remains confirmed when subsequent observation fails.
// 后续观测失败时，真实 RPC 成功仍保留已确认状态。
it("preserves confirmed completion alongside failed readback", async () => {
  await openAndConfirm(); read.mockRejectedValueOnce(new Error("query unavailable"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain(label("pluginRecovery.uninstallConfirmed"));
  expect(host.textContent).toContain("query unavailable"); expect(button("pluginRecovery.uninstallContinue").disabled).toBe(true);
  expect(resume).toHaveBeenCalledTimes(1);
});

// StrictMode's discarded mount cannot suppress the active read or fabricate a continuation.
// StrictMode 的丢弃挂载不能抑制活动读取或伪造继续。
it("retains read-only discovery through StrictMode remount", async () => {
  await act(async () => root.render(null)); await render({}, true);
  await act(async () => button("pluginRecovery.uninstallTitle").click());
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(false); expect(resume).not.toHaveBeenCalled();
});

// Disposing the renderer does not cancel or replay the service-owned original and still releases page admission.
// 销毁渲染器不取消或重放服务自有原实例，且仍释放页面入场。
it("releases admission after an in-flight continuation outlives the renderer", async () => {
  // finish completes the already dispatched original, not a new request.
  // finish 完成已派发原实例，而非新请求。
  let finish!: () => void;
  resume.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await openAndConfirm(); await act(async () => button("skills.plugins.confirm").click());
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await act(async () => button("skills.plugins.confirm").click()); expect(resume).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null)); read.mockResolvedValue([]); await act(async () => finish());
  expect(resume).toHaveBeenCalledExactlyOnceWith(original.pluginId, original.requestId, false);
  expect(release).toHaveBeenCalledTimes(1);
});
