// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { formatMessage, readLocaleMessage, type TranslationKey } from "@/i18n";
import prewarmWire from "@/service/__fixtures__/plugin-prewarm-policy-wire.json";
import runtimeWire from "@/service/__fixtures__/plugin-runtime-wire.json";
import type { PluginPendingContinuation, PluginPendingOperation } from "@/service/plugin-pending-operations-service";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-contract";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-contract";
import { capacityActionLabels, prewarmActionLabels } from "./plugin-policy-labels";
import { PluginRecoveryCenter, type RecoveryKind } from "./plugin-recovery-center";

vi.mock("@/i18n", async (importOriginal) => {
  // actual preserves authoritative translated button labels.
  // actual 保留权威翻译按钮标签。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ t: (key: TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});

// uninstall models a retained deletion whose plugin row has already disappeared from the catalog.
// uninstall 模拟插件行已从目录消失的保留删除。
const uninstallDetail: PluginUninstallStatus = { requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "hidden-plugin",
  phase: "collection", requiresExternalEffectsAcknowledgement: false };
// pendingUninstall lists detail with the offers the kernel declares for it.
// pendingUninstall 以内核为其声明的可用动作列出 detail。
function pendingUninstall(detail: PluginUninstallStatus, actions = [{ action: "resume", requiresAcknowledgement: detail.requiresExternalEffectsAcknowledgement }] as const)
  : PluginPendingOperation {
  return { kind: "uninstall", operationId: detail.requestId, pluginId: detail.pluginId, actions: [...actions], detail };
}
// updateDetail is a retained local update with recorded original effects.
// updateDetail 是带已记录原操作的保留本地更新。
const updateDetail = { requestId: "12345678-1234-4234-8234-123456789abd", pluginId: "fixture", phase: "committing", hasServiceOwner: false,
  candidate: { plugin_id: "fixture", generation_id: "12345678-1234-4234-8234-123456789abf", version: "2.0.0", content_fingerprint: "inspected" },
  activationRevision: "7", installation: "succeeded", effects: [{ operation: "probe", phase: "observation_failed" }] } as unknown as LocalPluginUpdateStatus;
// pendingUpdate lists detail with phase-appropriate offers supplied by the test as the kernel would declare them.
// pendingUpdate 以测试代为声明、符合阶段的可用动作列出 detail。
function pendingUpdate(detail: LocalPluginUpdateStatus, actions: PluginPendingOperation["actions"]): PluginPendingOperation {
  return { kind: "update", operationId: detail.requestId, pluginId: detail.pluginId, actions, detail } as PluginPendingOperation;
}
// capacity and prewarm are prepared policy receipts, the only state in which every policy continuation is offered.
// capacity 与 prewarm 是已准备策略回执，此状态下提供全部策略继续方式。
const capacity = { kind: "capacityPolicy", operationId: runtimeWire.runtime.policyUpdate.request.updateId, pluginId: runtimeWire.runtime.policyUpdate.request.pluginId,
  detail: { ...runtimeWire.runtime.policyUpdate, phase: "prepared" },
  actions: [{ action: "cancel", requiresAcknowledgement: false }, { action: "resume", requiresAcknowledgement: false }] } as unknown as PluginPendingOperation;
const prewarm = { kind: "prewarmPolicy", operationId: prewarmWire.published.update.request.updateId, pluginId: null,
  detail: prewarmWire.published.update, actions: [{ action: "forget", requiresAcknowledgement: false }] } as unknown as PluginPendingOperation;

// read observes the whole registry; continue records each exact continuation.
// read 观测整个登记表；continue 记录每个精确继续请求。
const read = vi.fn<() => Promise<PluginPendingOperation[]>>();
const resume = vi.fn<(continuation: PluginPendingContinuation) => Promise<void>>();
// acquire and release model the synchronous page gate shared with ordinary plugin controls.
// acquire 与 release 模拟与普通插件控件共享的同步页面门。
const acquire = vi.fn<() => boolean>();
const release = vi.fn<() => void>();
// confirmed receives only an actual successful uninstall continuation or an explicit dismissal.
// confirmed 仅接收实际成功的卸载继续或明确放弃追踪。
const confirmed = vi.fn<(pluginId: string) => void>();
let host: HTMLDivElement;
let root: Root;
// scanToken and current mirror the re-check token and the props of the latest render.
// scanToken 及 current 对应重新检查令牌及最近一次渲染的属性。
let scanToken = 0;
type Overrides = Partial<React.ComponentProps<typeof PluginRecoveryCenter>>;
let current: { overrides: Overrides; strict: boolean } = { overrides: {}, strict: false };
const allKinds: RecoveryKind[] = ["update", "uninstall", "capacityPolicy", "prewarmPolicy"];

// label resolves the English catalog text for key, filling the fallback plugin name.
// label 解析 key 的英文目录文本，并填入回退插件名称。
function label(key: TranslationKey, name = uninstallDetail.pluginId): string {
  return formatMessage(readLocaleMessage("en-US", key), { name, version: "2.0.0" });
}
// button finds a control by its semantic label; throws if the fixture contract has changed.
// button 按语义标签查找控件；夹具契约变化时抛错。
function button(key: TranslationKey, name?: string): HTMLButtonElement {
  const found = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent === label(key, name));
  if (!found) throw new Error(`Expected ${label(key, name)}; its label or availability may have changed`);
  return found;
}
// dialog returns the open confirmation portal, or null.
// dialog 返回已打开的确认门户，或 null。
const dialog = () => document.querySelector('[role="alertdialog"]');
// render mounts the center with every kind visible; returns after the mount read.
// render 以全部类别可见挂载中心；挂载读取后返回。
async function render(overrides: Overrides = {}, strict = false): Promise<void> {
  const center = <PluginRecoveryCenter actions={{ onReadPluginPendingOperations: read, onContinuePluginPendingOperation: resume }}
    gate={{ mutationBusy: false, acquireMutation: acquire, releaseMutation: release }} kinds={allKinds} onUninstallConfirmed={confirmed}
    pluginName={(pluginId) => pluginId} scanToken={scanToken} uninstallUncertainty={null} {...overrides} />;
  current = { overrides, strict };
  await act(async () => root.render(strict ? <React.StrictMode>{center}</React.StrictMode> : center));
}
// remount discards the mounted center and renders a fresh one.
// remount 丢弃已挂载中心并渲染新的中心。
async function remount(overrides: Overrides = {}, strict = false): Promise<void> {
  await act(async () => root.render(null)); await render(overrides, strict);
}
// recheck asks the mounted center to read again, as the maintenance re-check button does.
// recheck 以维护重新检查按钮的方式要求已挂载中心再次读取。
async function recheck(): Promise<void> { scanToken += 1; await render(current.overrides, current.strict); }
// click activates button inside React's act scope.
// click 在 React act 范围内激活按钮。
async function click(target: HTMLButtonElement): Promise<void> { await act(async () => target.click()); }

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks(); scanToken = 0;
  read.mockResolvedValue([pendingUninstall(uninstallDetail)]); resume.mockResolvedValue(); acquire.mockReturnValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host); await render();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// One mount read discovers an uninstall whose plugin is hidden; RPC success is authoritative even when the record disappears.
// 单次挂载读取发现插件已隐藏的卸载；即使记录消失，RPC 成功仍具权威。
it("discovers hidden originals and confirms only a successful exact continuation", async () => {
  expect(read).toHaveBeenCalledExactlyOnceWith(); expect(resume).not.toHaveBeenCalled();
  await click(button("pluginRecovery.uninstallContinue"));
  expect(dialog()?.textContent).toContain(uninstallDetail.requestId);
  expect(dialog()?.querySelector('[role="checkbox"]')).toBeNull();
  read.mockResolvedValue([]); await click(button("skills.plugins.confirm"));
  expect(resume).toHaveBeenCalledExactlyOnceWith({ kind: "uninstall", operationId: uninstallDetail.requestId, pluginId: uninstallDetail.pluginId,
    action: "resume", acknowledgeExternalEffects: false });
  expect(confirmed).toHaveBeenCalledExactlyOnceWith(uninstallDetail.pluginId); expect(release).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(label("pluginRecovery.uninstallConfirmed"));
});

// Only a declared acknowledgement requirement shows the box, which starts unchecked and gates the confirmation.
// 仅声明需要确认时显示勾选框，其初始未勾选并控制确认。
it("shows acknowledgement only when required and sends true only after an explicit check", async () => {
  read.mockResolvedValue([pendingUninstall({ ...uninstallDetail, requiresExternalEffectsAcknowledgement: true })]); await recheck();
  await click(button("pluginRecovery.uninstallContinue"));
  const checkbox = dialog()?.querySelector<HTMLButtonElement>('[role="checkbox"]');
  if (!checkbox) throw new Error("Expected acknowledgement; the declared requirement may have changed");
  expect(checkbox.getAttribute("aria-checked")).toBe("false");
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await click(button("skills.plugins.confirm")); expect(resume).not.toHaveBeenCalled();
  await click(checkbox); read.mockResolvedValue([]); await click(button("skills.plugins.confirm"));
  expect(resume).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: "uninstall", acknowledgeExternalEffects: true }));
});

// A lost response stays unknown while the operation is unseen; rediscovery re-enables an explicit retry without replay.
// 响应丢失时，在未再次看到操作前保持未知；再次发现后重新允许明确重试，不自动重放。
it("keeps a lost continuation unknown and permits an explicit retry only after rediscovery", async () => {
  await click(button("pluginRecovery.uninstallContinue")); resume.mockRejectedValueOnce(new Error("response lost")); read.mockRejectedValue(new Error("query unavailable"));
  await click(button("skills.plugins.confirm"));
  expect(host.textContent).toContain("response lost"); expect(host.textContent).toContain(label("pluginRecovery.uninstallUnknownNotice"));
  expect(confirmed).not.toHaveBeenCalled(); expect(() => button("pluginRuntime.discardTracking")).toThrow();
  await recheck(); expect(host.textContent).toContain(label("pluginRecovery.uninstallUnknownNotice"));
  read.mockResolvedValue([pendingUninstall(uninstallDetail)]); await recheck();
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(false); expect(resume).toHaveBeenCalledTimes(1);
  expect(host.textContent).not.toContain(label("pluginRecovery.uninstallUnknownNotice"));
});

// Once a successful read no longer lists the operation, only the user's explicit dismissal stops tracking it.
// 成功读取不再列出该操作后，仅用户明确放弃追踪才停止追踪。
it("offers stop tracking only after a successful read no longer lists the operation", async () => {
  await click(button("pluginRecovery.uninstallContinue")); resume.mockRejectedValueOnce(new Error("response lost")); read.mockResolvedValue([]);
  await click(button("skills.plugins.confirm"));
  expect(host.textContent).toContain(label("pluginRecovery.unconfirmedGone")); expect(confirmed).not.toHaveBeenCalled();
  await recheck(); expect(host.textContent).toContain(label("pluginRecovery.unconfirmedGone"));
  await click(button("pluginRuntime.discardTracking"));
  expect(confirmed).toHaveBeenCalledExactlyOnceWith(uninstallDetail.pluginId); expect(resume).toHaveBeenCalledTimes(1);
  expect(host.textContent).not.toContain(label("pluginRecovery.unconfirmedGone"));
});

// A foreground uninstall without a captured UUID is never treated as finished by an empty registry.
// 未捕获 UUID 的前台卸载绝不因登记表为空而被视为完成。
it("keeps the foreground no-ID uncertainty across an empty registry", async () => {
  read.mockResolvedValue([]); await remount({ uninstallUncertainty: { pluginId: uninstallDetail.pluginId, requestId: null } });
  expect(host.textContent).toContain(label("pluginRecovery.unconfirmedGone"));
  expect(resume).not.toHaveBeenCalled(); expect(confirmed).not.toHaveBeenCalled();
});

// A competing ordinary mutation keeps admission closed, and a row the service offers nothing for has no button.
// 竞争的普通变更保持入场关闭，服务未提供动作的行没有按钮。
it("respects the shared page gate and rows without declared offers", async () => {
  await click(button("pluginRecovery.uninstallContinue")); acquire.mockReturnValue(false); await click(button("skills.plugins.confirm"));
  expect(resume).not.toHaveBeenCalled(); expect(release).not.toHaveBeenCalled();
  read.mockResolvedValue([pendingUninstall({ ...uninstallDetail, phase: "executing" }, [] as never)]); await remount();
  expect(host.textContent).toContain(label("pluginRecovery.executing"));
  expect(() => button("pluginRecovery.uninstallContinue")).toThrow();
});

// A real RPC success stays confirmed when the following readback fails.
// 后续回读失败时，真实 RPC 成功仍保持已确认。
it("preserves confirmed completion alongside a failed readback", async () => {
  await click(button("pluginRecovery.uninstallContinue")); read.mockRejectedValueOnce(new Error("query unavailable"));
  await click(button("skills.plugins.confirm"));
  expect(host.textContent).toContain(label("pluginRecovery.uninstallConfirmed")); expect(host.textContent).toContain("query unavailable");
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(true); expect(resume).toHaveBeenCalledTimes(1);
});

// StrictMode's discarded mount cannot suppress the active read or fabricate a continuation.
// StrictMode 的丢弃挂载不能抑制活动读取或伪造继续。
it("retains read-only discovery through StrictMode remount", async () => {
  await remount({}, true);
  expect(button("pluginRecovery.uninstallContinue").disabled).toBe(false); expect(resume).not.toHaveBeenCalled();
});

// Disposing the renderer neither cancels nor replays the service-owned operation and still releases page admission.
// 销毁渲染器既不取消也不重放服务自有操作，且仍释放页面入场。
it("releases admission after an in-flight continuation outlives the renderer", async () => {
  let finish!: () => void;
  resume.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await click(button("pluginRecovery.uninstallContinue")); await click(button("skills.plugins.confirm"));
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await click(button("skills.plugins.confirm")); expect(resume).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null)); read.mockResolvedValue([]); await act(async () => finish());
  expect(resume).toHaveBeenCalledTimes(1); expect(release).toHaveBeenCalledTimes(1);
});

// A plugin drawer narrows the rows by owner and kind while the full read and its checks stay unchanged.
// 插件抽屉按所有者与类别缩小显示行，全量读取及其校验保持不变。
it("narrows displayed rows to the scoped plugin and kinds", async () => {
  // reported receives every uninstall record of the full read, independent of the displayed scope.
  // reported 接收全量读取的每条卸载记录，与显示范围无关。
  const reported = vi.fn();
  const other = { ...uninstallDetail, requestId: "12345678-1234-4234-8234-123456789abe", pluginId: "other" };
  read.mockResolvedValue([pendingUninstall(uninstallDetail), pendingUninstall(other), capacity, prewarm]);
  await remount({ scopePluginId: "other", kinds: ["uninstall"], onUninstallRecordsObserved: reported });
  expect(host.querySelectorAll("article")).toHaveLength(1);
  expect(host.textContent).toContain(label("pluginRecovery.uninstallItemTitle", "other"));
  expect(reported).toHaveBeenLastCalledWith([uninstallDetail, other]);
});

// Update rows name their phase, show recorded effects, and review abandonment in a dialog before dispatch.
// 更新行显示其阶段与已记录操作，并在派发前于对话框中审阅放弃。
it("reviews update abandonment in a dialog and resumes without one", async () => {
  const update = pendingUpdate(updateDetail, [{ action: "abort", requiresAcknowledgement: true }, { action: "resume", requiresAcknowledgement: false }]);
  read.mockResolvedValue([update]); await remount();
  expect(host.textContent).toContain(label("pluginUpdates.committing")); expect(host.textContent).toContain(label("pluginUpdates.probe"));
  expect(host.textContent).toContain(label("pluginUpdates.installUncertain"));
  await click(button("pluginUpdates.abandon"));
  expect(dialog()?.textContent).toContain(label("pluginUpdates.abandonWarning")); expect(resume).not.toHaveBeenCalled();
  await click(button("skills.plugins.confirm"));
  expect(resume).toHaveBeenLastCalledWith({ kind: "update", operationId: updateDetail.requestId, pluginId: "fixture", action: "abort", acknowledgeExternalEffects: true });
  expect(host.textContent).toContain(label("pluginUpdates.finishedAbort", "fixture"));
  await click(button("pluginUpdates.resume")); expect(dialog()).toBeNull();
  expect(resume).toHaveBeenLastCalledWith({ kind: "update", operationId: updateDetail.requestId, pluginId: "fixture", action: "resume", acknowledgeExternalEffects: false });
});

// An aborted update's remaining step is cleanup, which is reviewed with its own warning.
// 已放弃更新的剩余步骤是清理，以其专属警告审阅。
it("names cleanup of an aborted update and warns before it runs", async () => {
  read.mockResolvedValue([pendingUpdate({ ...updateDetail, phase: "aborted" }, [{ action: "abort", requiresAcknowledgement: true }])]); await remount();
  await click(button("pluginUpdates.finishCleanup"));
  expect(dialog()?.textContent).toContain(label("pluginUpdates.cleanupWarning")); expect(resume).not.toHaveBeenCalled();
});

// Policy rows render exactly the declared offers and continue without a dialog; prewarm has no plugin owner.
// 策略行精确渲染已声明可用动作并无需对话框继续；预热策略没有插件所有者。
it("renders declared policy offers and continues them directly", async () => {
  read.mockResolvedValue([capacity, prewarm]); await remount();
  // offers lists one row's policy offer labels; rows are found by their operation ID, never by position.
  // offers 列出单行策略可用动作标签；按操作 ID 查找行，绝不依赖位置。
  const policyLabels = new Set([...Object.values(capacityActionLabels), ...Object.values(prewarmActionLabels)].map(([key]) => label(key)));
  const row = (operationId: string) => [...host.querySelectorAll("article")].find((candidate) => candidate.textContent?.includes(operationId));
  const offers = (operationId: string) => [...row(operationId)?.querySelectorAll("button") ?? []].filter((candidate) => policyLabels.has(candidate.textContent ?? ""));
  expect(offers(capacity.operationId).map((candidate) => candidate.textContent))
    .toEqual([capacityActionLabels.cancel, capacityActionLabels.resume].map(([key]) => label(key)));
  expect(offers(prewarm.operationId).map((candidate) => candidate.textContent)).toEqual([label(prewarmActionLabels.forget[0])]);
  await click(offers(capacity.operationId)[0]);
  expect(resume).toHaveBeenLastCalledWith({ kind: "capacityPolicy", operationId: capacity.operationId, action: "cancel" });
  expect(host.textContent).toContain(label("pluginRecovery.policyFinished", capacity.pluginId!));
  await click(offers(prewarm.operationId)[0]); expect(dialog()).toBeNull();
  expect(resume).toHaveBeenLastCalledWith({ kind: "prewarmPolicy", operationId: prewarm.operationId, action: "forget" });
  expect(host.textContent).toContain(label("pluginRecovery.policyFinished", readLocaleMessage("en-US", "pluginMaintenance.prewarm.title")));
});
