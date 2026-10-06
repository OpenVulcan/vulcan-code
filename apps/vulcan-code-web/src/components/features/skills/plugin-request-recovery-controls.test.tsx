// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { formatMessage, readLocaleMessage, type TranslationKey } from "@/i18n";
import type { PluginSecretMutationStatus } from "@/service/plugin-secret-recovery-service";
import type { RecoveryReport } from "./plugin-maintenance-section";
import { PluginRequestRecoveryControls } from "./plugin-request-recovery-controls";

vi.mock("@/i18n", async (importOriginal) => {
  // Preserve actual English translations instead of synthetic test-only button labels.
  // 保留实际英文翻译，而非合成测试专用按钮标签。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});

// original exposes only manifest identities of a retained credential mutation, never a credential or preimage value.
// original 仅暴露保留凭据变更的清单身份，绝不包含凭据或前像值。
const original: PluginSecretMutationStatus = {
  requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "unpublished", phase: "nativeRefresh", serverId: "server", secretKey: "token",
};
// readSecret observes original non-secret credential mutation identities without side effects.
// readSecret 无副作用观测原无秘密凭据变更身份。
const readSecret = vi.fn<(pluginId: string) => Promise<PluginSecretMutationStatus[]>>();
// recoverSecret records the exact two-argument credential recovery call.
// recoverSecret 记录精确双参数凭据恢复调用。
const recoverSecret = vi.fn<(pluginId: string, requestId: string) => Promise<void>>();
// acquire and release model the existing parent page's synchronous mutation admission.
// acquire 及 release 模拟既有父页面同步变更入场。
const acquire = vi.fn<() => boolean>();
const release = vi.fn<() => void>();
// report observes what the reader tells its recovery center.
// report 观测读取器告知恢复中心的内容。
const report = vi.fn<(next: RecoveryReport) => void>();
let host: HTMLDivElement;
let root: Root;
let scanToken = 0;

// label resolves the actual supported English message, filling the plugin name when the message has one.
// label 解析实际支持英文文案，含插件名称占位时填入名称。
function label(key: TranslationKey): string { return formatMessage(readLocaleMessage("en-US", key), { name: "Unpublished" }); }

// button locates a semantic visible label; returns its element or an explicit stale-fixture error.
// button 按语义可见标签定位；返回其元素或明确过期夹具错误。
function button(key: TranslationKey): HTMLButtonElement {
  // found selects by actual text, independently from layout indexes.
  // found 按实际文本选择，独立于布局下标。
  const found = [...document.querySelectorAll("button")].find((element) => element.textContent === label(key));
  if (!found) throw new Error(`Expected ${label(key)}; its current label or availability may have changed`);
  return found;
}

// render mounts the credential reader for one exact owner; mounting performs the first read.
// render 为单个精确所有者挂载凭据读取器；挂载即执行首次读取。
async function render(): Promise<void> {
  await act(async () => root.render(<PluginRequestRecoveryControls pluginId="unpublished" pluginName="Unpublished"
    actions={{ onReadPluginSecretMutations: readSecret, onRecoverPluginSecretMutation: recoverSecret }}
    mutationBusy={false} acquireMutation={acquire} releaseMutation={release} onReport={report} scanToken={scanToken} />));
}

// recheck asks the reader to check again, the way the recovery center's re-check button does.
// recheck 以恢复中心重新检查按钮的方式要求读取器再次检查。
async function recheck(): Promise<void> {
  scanToken += 1; await render();
}

// recoverAction names the credential rollback offered for the fixture's non-committed phase.
// recoverAction 命名夹具非提交阶段所提供的凭据回滚操作。
const recoverAction: TranslationKey = "pluginRecovery.action.rollbackSecret";

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks(); scanToken = 0;
  readSecret.mockResolvedValue([original]); recoverSecret.mockResolvedValue(); acquire.mockReturnValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Credential mutations are recoverable only through their read original UUID, after an explicit confirmation.
// 凭据变更仅通过已读取的原 UUID 恢复，且须明确确认。
it("reads on mount and recovers the exact UUID after confirmation", async () => {
  await render();
  expect(readSecret).toHaveBeenCalledExactlyOnceWith("unpublished"); expect(recoverSecret).not.toHaveBeenCalled();
  expect(report).toHaveBeenLastCalledWith({ loading: false, visible: 1 });
  expect(host.textContent).toContain("server / token");
  await act(async () => button(recoverAction).click());
  expect(recoverSecret).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  readSecret.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverSecret).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId);
  expect(acquire).toHaveBeenCalledTimes(1); expect(release).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(label("pluginRecovery.confirmed"));
  expect(host.textContent).not.toContain(label("pluginRecovery.unknownNotice"));
  expect(host.querySelector('input[type="password"]')).toBeNull();
});

// An owner with nothing retained reports an empty, settled reader so the center can show its all-clear state.
// 无保留内容的所有者报告空且已完成的读取器，使中心可显示一切正常状态。
it("reports nothing to show when no original is retained", async () => {
  readSecret.mockResolvedValue([]); await render();
  expect(report).toHaveBeenLastCalledWith({ loading: false, visible: 0 });
  expect(host.querySelector("article")).toBeNull();
});

// Active exclusive borrows cannot be recovered concurrently, regardless of catalog visibility.
// 活动排他借用不能并发恢复，无论目录可见性如何。
it("offers no recovery for executing originals", async () => {
  readSecret.mockResolvedValue([{ ...original, phase: "executing" }]); await render();
  expect(host.querySelector("article")?.textContent).toContain(label("pluginRecovery.executing"));
  // Only the technical-details disclosure remains; no rollback, cleanup or finish action is offered.
  // 仅保留技术详情展开按钮；不提供回滚、清理或完成操作。
  expect([...host.querySelectorAll("article button")].filter((element) => element.getAttribute("data-slot") !== "inline-disclosure-trigger")).toEqual([]);
  expect(acquire).not.toHaveBeenCalled();
});

// A foreign namespace or duplicate UUID response cannot grant recovery admission.
// 外部命名空间或重复 UUID 响应不能授予恢复入场。
it.each(["foreign", "duplicate"] as const)("rejects %s original observations", async (kind) => {
  readSecret.mockResolvedValue(kind === "foreign" ? [{ ...original, pluginId: "another" }] : [original, original]);
  await render(); expect(host.querySelector('[role="alert"]')?.textContent).toContain(label("pluginRecovery.identityMismatch"));
  expect(host.querySelector("article")).toBeNull(); expect(recoverSecret).not.toHaveBeenCalled();
});

// A reply that arrives after the reader is gone is discarded instead of granting recovery.
// 读取器消失后到达的回复被丢弃，而非授予恢复。
it("ignores late responses after disposal", async () => {
  // finish is the completion of the one actual-shaped deferred query, not a replacement read.
  // finish 是单个符合实际结构延迟查询的完成者，不是替代读取。
  let finish!: (records: PluginSecretMutationStatus[]) => void;
  readSecret.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await render(); await act(async () => root.render(null));
  await act(async () => finish([original])); expect(host.querySelector("article")).toBeNull();
  expect(recoverSecret).not.toHaveBeenCalled();
});

// A failed foreground and missing original remain uncertain across re-checks until their exact record is read again.
// 失败前台及缺失原实例跨重新检查保持不确定，直到其精确记录再次被读取。
it("retains uncertain originals until their exact record is read again", async () => {
  await render(); await act(async () => button(recoverAction).click());
  recoverSecret.mockRejectedValueOnce(new Error("response lost")); readSecret.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain("response lost");
  expect(host.textContent).toContain(label("pluginRecovery.unknownNotice"));
  readSecret.mockResolvedValue([{ ...original, requestId: "12345678-1234-4234-8234-123456789abd" }]);
  await recheck(); expect(button(recoverAction).disabled).toBe(true);
  expect(host.textContent).toContain(label("pluginRecovery.unknownNotice"));
  readSecret.mockResolvedValue([original]);
  await recheck(); expect(button(recoverAction).disabled).toBe(false);
  expect(host.textContent).not.toContain(label("pluginRecovery.unknownNotice"));
  expect(recoverSecret).toHaveBeenCalledTimes(1);
});

// Explicit success remains authoritative if its subsequent read fails; no automatic recovery replay follows.
// 明确成功在后续读取失败时仍具权威；不发生自动恢复重放。
it("preserves an acknowledged recovery alongside a failed readback", async () => {
  await render(); await act(async () => button(recoverAction).click());
  readSecret.mockRejectedValueOnce(new Error("query unavailable"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain(label("pluginRecovery.confirmed"));
  expect(host.textContent).toContain("query unavailable"); expect(button(recoverAction).disabled).toBe(true);
  expect(recoverSecret).toHaveBeenCalledTimes(1); expect(release).toHaveBeenCalledTimes(1);
});

// Failed recovery may be explicitly retried after the same retained record is read, without hiding its original error.
// 失败恢复在读取相同保留记录后可明确重试，不隐藏原错误。
it("keeps a failed recovery visible and permits only an explicit retry of the original UUID", async () => {
  await render(); await act(async () => button(recoverAction).click());
  recoverSecret.mockRejectedValueOnce(new Error("native still occupied"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain("native still occupied"); expect(button(recoverAction).disabled).toBe(false);
  expect(recoverSecret).toHaveBeenCalledTimes(1);
  await act(async () => button(recoverAction).click()); readSecret.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverSecret).toHaveBeenCalledTimes(2); expect(recoverSecret).toHaveBeenLastCalledWith("unpublished", original.requestId);
});

// Shared page admission refuses a competing action before any recovery RPC or uncertain state is created.
// 共享页面入场在任何恢复 RPC 或不确定状态创建之前拒绝竞争动作。
it("respects the same synchronous parent mutation gate", async () => {
  await render(); await act(async () => button(recoverAction).click()); acquire.mockReturnValue(false);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverSecret).not.toHaveBeenCalled(); expect(release).not.toHaveBeenCalled();
  expect(host.textContent).not.toContain(label("pluginRecovery.unknownNotice"));
});

// In-flight recovery cannot be repeated, and disposing the renderer releases only page admission.
// 进行中恢复不能重复，销毁渲染器仅释放页面入场。
it("locks recovery while in flight and releases admission after disposal without replay", async () => {
  // finish resolves the same already dispatched recovery without cancelling its service work.
  // finish 结束相同已派发恢复，不取消其服务工作。
  let finish!: () => void;
  recoverSecret.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await render(); await act(async () => button(recoverAction).click());
  await act(async () => button("skills.plugins.confirm").click());
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await act(async () => button("skills.plugins.confirm").click()); expect(recoverSecret).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null)); readSecret.mockResolvedValue([]);
  await act(async () => finish()); expect(release).toHaveBeenCalledTimes(1);
  expect(recoverSecret).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId);
});
