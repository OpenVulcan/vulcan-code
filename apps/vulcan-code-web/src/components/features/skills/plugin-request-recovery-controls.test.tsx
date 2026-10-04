// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { readLocaleMessage } from "@/i18n";
import type { PluginInstallStatus } from "@/service/plugin-install-recovery-service";
import type { PluginSecretMutationStatus } from "@/service/plugin-secret-recovery-service";
import { PluginRequestRecoveryControls } from "./plugin-request-recovery-controls";

vi.mock("@/i18n", async (importOriginal) => {
  // Preserve actual English translations instead of synthetic test-only button labels.
  // 保留实际英文翻译，而非合成测试专用按钮标签。
  const actual = await importOriginal<typeof import("@/i18n")>();
  return { ...actual, useI18n: () => ({ t: (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key) }) };
});

// original identifies an actual-shaped unpublished retained install independently from a catalog row.
// original 独立于目录行标识符合实际结构的尚未发布保留安装。
const original: PluginInstallStatus = { requestId: "12345678-1234-4234-8234-123456789abc", pluginId: "unpublished", phase: "nativeValidation" };
// secretOriginal exposes only manifest identities, never a credential or preimage value.
// secretOriginal 仅暴露清单身份，绝不包含凭据或前像值。
const secretOriginal: PluginSecretMutationStatus = { ...original, phase: "nativeRefresh", serverId: "server", secretKey: "token" };
// readInstall observes retained original requests without side effects.
// readInstall 无副作用观测保留原请求。
const readInstall = vi.fn<(pluginId: string) => Promise<PluginInstallStatus[]>>();
// recoverInstall records exact arguments including explicit compensator acknowledgement.
// recoverInstall 记录精确参数，包括明确补偿确认。
const recoverInstall = vi.fn<(pluginId: string, requestId: string, acknowledgement: boolean) => Promise<void>>();
// readSecret observes original non-secret credential mutation identities.
// readSecret 观测原无秘密凭据变更身份。
const readSecret = vi.fn<(pluginId: string) => Promise<PluginSecretMutationStatus[]>>();
// recoverSecret exercises the independent two-argument credential recovery protocol.
// recoverSecret 验证独立双参数凭据恢复协议。
const recoverSecret = vi.fn<(pluginId: string, requestId: string) => Promise<void>>();
// acquire and release model the existing parent page's synchronous mutation admission.
// acquire 及 release 模拟既有父页面同步变更入场。
const acquire = vi.fn<() => boolean>();
// release observes unconditional admission release even when the component is disposed.
// release 观测无条件入场释放，包括组件销毁之后。
const release = vi.fn<() => void>();
// host owns a real DOM fixture with no installed plugin rows.
// host 拥有不含已安装插件行的真实 DOM 夹具。
let host: HTMLDivElement;
// root owns React rendering and actual confirmation portal cleanup.
// root 拥有 React 渲染及实际确认门户清理。
let root: Root;

// label resolves the actual supported English message; returns no invented test copy.
// label 解析实际支持英文文案；不返回虚构测试文案。
function label(key: import("@/i18n").TranslationKey): string { return readLocaleMessage("en-US", key); }

// button locates a semantic visible label; returns its element or an explicit stale-fixture error.
// button 按语义可见标签定位；返回其元素或明确过期夹具错误。
function button(key: import("@/i18n").TranslationKey): HTMLButtonElement {
  // found selects by actual text, independently from layout indexes.
  // found 按实际文本选择，独立于布局下标。
  const found = [...document.querySelectorAll("button")].find((element) => element.textContent === label(key));
  if (!found) throw new Error(`Expected ${label(key)}; its current label or availability may have changed`);
  return found;
}

// render mounts one declared service protocol with shared admission and no catalog membership.
// render 挂载一个声明服务协议及共享入场，不依赖目录成员关系。
// kind selects installation or credential; returns after React commits.
// kind 选择安装或凭据；在 React 提交后返回。
async function render(kind: "installation" | "credential" = "installation"): Promise<void> {
  await act(async () => root.render(<PluginRequestRecoveryControls kind={kind} plugins={[]} suggestedPluginId="unpublished"
    actions={{ onReadPluginInstallRequests: readInstall, onRecoverPluginInstallRequest: recoverInstall,
      onReadPluginSecretMutations: readSecret, onRecoverPluginSecretMutation: recoverSecret }}
    mutationBusy={false} acquireMutation={acquire} releaseMutation={release} />));
}

// openAndRead expands the chosen panel and explicitly queries its original namespace; never mutates.
// openAndRead 展开选定面板并明确查询原命名空间；绝不变更。
async function openAndRead(kind: "installation" | "credential" = "installation"): Promise<void> {
  await act(async () => button(kind === "installation" ? "pluginRecovery.installTitle" : "pluginRecovery.secretTitle").click());
  await act(async () => button("pluginRecovery.read").click());
}

// enterOwner delivers a real React input event; returns after the new identity invalidates old observations.
// enterOwner 发送真实 React 输入事件；在新身份使旧观测失效之后返回。
async function enterOwner(owner: string): Promise<void> {
  // input is the only explicit namespace field, located semantically rather than by layout position.
  // input 是唯一明确命名空间字段，按语义而非布局位置定位。
  const input = host.querySelector<HTMLInputElement>("input[list]");
  if (!input) throw new Error("Expected explicit plugin field; its current structure may have changed");
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, owner);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  readInstall.mockResolvedValue([original]); readSecret.mockResolvedValue([secretOriginal]);
  recoverInstall.mockResolvedValue(); recoverSecret.mockResolvedValue(); acquire.mockReturnValue(true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  await render();
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Unpublished installations are recoverable only through their explicitly read original UUID.
// 尚未发布安装仅通过明确读取的原 UUID 恢复。
it("queries unpublished originals and recovers the exact UUID with acknowledgement false", async () => {
  expect(readInstall).not.toHaveBeenCalled(); expect(recoverInstall).not.toHaveBeenCalled();
  await openAndRead();
  expect(readInstall).toHaveBeenCalledExactlyOnceWith("unpublished");
  await act(async () => button("pluginRecovery.recover").click());
  expect(recoverInstall).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain(original.requestId);
  readInstall.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverInstall).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId, false);
  expect(recoverSecret).not.toHaveBeenCalled(); expect(acquire).toHaveBeenCalledTimes(1); expect(release).toHaveBeenCalledTimes(1);
  expect(host.textContent).toContain(label("pluginRecovery.confirmed"));
  expect(host.textContent).not.toContain(label("pluginRecovery.unknown"));
});

// Credential recovery uses its own registry and carries no installation acknowledgement or secret values.
// 凭据恢复使用自身注册表，不携带安装确认或秘密值。
it("recovers only the original credential protocol and renders public server and key identities", async () => {
  await render("credential"); await openAndRead("credential");
  expect(host.textContent).toContain("server / token");
  await act(async () => button("pluginRecovery.recover").click());
  expect(document.querySelector('[role="alertdialog"] [role="checkbox"]')).toBeNull();
  readSecret.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverSecret).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId);
  expect(readInstall).not.toHaveBeenCalled(); expect(recoverInstall).not.toHaveBeenCalled();
  expect(host.querySelector('input[type="password"]')).toBeNull();
});

// The compensator acknowledgement must be a separate checked user decision, never inferred from a phase.
// 补偿确认必须是独立勾选的用户决定，绝不从阶段推断。
it("passes true only after the user explicitly checks compensator acknowledgement", async () => {
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click());
  // checkbox exists only for this noncommitted original installation.
  // checkbox 仅为此尚未提交原安装存在。
  const checkbox = document.querySelector<HTMLButtonElement>('[role="alertdialog"] [role="checkbox"]');
  if (!checkbox) throw new Error("Expected separate compensator acknowledgement; its contract may have changed");
  expect(checkbox.getAttribute("aria-checked")).toBe("false");
  await act(async () => checkbox.click()); readInstall.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverInstall).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId, true);
});

// Active exclusive borrows cannot be recovered concurrently, regardless of catalog visibility.
// 活动排他借用不能并发恢复，无论目录可见性如何。
it("disables recovery for executing originals", async () => {
  readInstall.mockResolvedValue([{ ...original, phase: "executing" }]); await openAndRead();
  expect(button("pluginRecovery.recover").disabled).toBe(true);
  expect(host.textContent).toContain(label("pluginRecovery.executing")); expect(acquire).not.toHaveBeenCalled();
});

// A foreign namespace or duplicate UUID response cannot grant recovery admission.
// 外部命名空间或重复 UUID 响应不能授予恢复入场。
it.each(["foreign", "duplicate"] as const)("rejects %s original observations", async (kind) => {
  readInstall.mockResolvedValue(kind === "foreign" ? [{ ...original, pluginId: "another" }] : [original, original]);
  await openAndRead(); expect(host.querySelector('[role="alert"]')?.textContent).toContain(label("pluginRecovery.identityMismatch"));
  expect(host.querySelector("article")).toBeNull(); expect(recoverInstall).not.toHaveBeenCalled();
});

// Editing while a read awaits invalidates its response instead of retargeting original recovery.
// 读取等待期间编辑使响应失效，而非重新定向原恢复。
it("ignores late responses after the explicit namespace changes", async () => {
  // finish is the completion of the one actual-shaped deferred query, not a replacement read.
  // finish 是单个符合实际结构延迟查询的完成者，不是替代读取。
  let finish!: (records: PluginInstallStatus[]) => void;
  readInstall.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await openAndRead(); await enterOwner("another");
  await act(async () => finish([original])); expect(host.querySelector("article")).toBeNull();
  expect(recoverInstall).not.toHaveBeenCalled();
});

// A failed foreground and missing original remain uncertain across empty reads, owner changes and collapse.
// 失败前台及缺失原实例跨空读取、所有者变化及折叠保持不确定。
it("retains uncertain originals until their exact record is read again", async () => {
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click());
  recoverInstall.mockRejectedValueOnce(new Error("response lost")); readInstall.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain("response lost"); expect(host.textContent).toContain(original.requestId);
  expect(host.textContent).toContain(label("pluginRecovery.unknown"));
  await act(async () => button("pluginRecovery.installTitle").click());
  await act(async () => button("pluginRecovery.installTitle").click());
  await enterOwner("another"); readInstall.mockResolvedValue([{ ...original, pluginId: "another", requestId: "12345678-1234-4234-8234-123456789abd" }]);
  await act(async () => button("pluginRecovery.read").click()); expect(button("pluginRecovery.recover").disabled).toBe(true);
  await enterOwner("unpublished"); readInstall.mockResolvedValue([original]);
  await act(async () => button("pluginRecovery.read").click()); expect(button("pluginRecovery.recover").disabled).toBe(false);
  expect(recoverInstall).toHaveBeenCalledTimes(1);
});

// Explicit success remains authoritative if its subsequent read fails; no automatic recovery replay follows.
// 明确成功在后续读取失败时仍具权威；不发生自动恢复重放。
it("preserves an acknowledged recovery alongside a failed readback", async () => {
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click());
  readInstall.mockRejectedValueOnce(new Error("query unavailable"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain(label("pluginRecovery.confirmed"));
  expect(host.textContent).toContain("query unavailable"); expect(button("pluginRecovery.recover").disabled).toBe(true);
  expect(recoverInstall).toHaveBeenCalledTimes(1); expect(release).toHaveBeenCalledTimes(1);
});

// Failed recovery may be explicitly retried after the same retained record is read, without hiding its original error.
// 失败恢复在读取相同保留记录后可明确重试，不隐藏原错误。
it("keeps a failed recovery visible and permits only an explicit retry of the original UUID", async () => {
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click());
  recoverInstall.mockRejectedValueOnce(new Error("native still occupied"));
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.textContent).toContain("native still occupied"); expect(button("pluginRecovery.recover").disabled).toBe(false);
  expect(recoverInstall).toHaveBeenCalledTimes(1);
  await act(async () => button("pluginRecovery.recover").click()); readInstall.mockResolvedValue([]);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverInstall).toHaveBeenCalledTimes(2); expect(recoverInstall).toHaveBeenLastCalledWith("unpublished", original.requestId, false);
});

// Shared page admission refuses a competing action before any recovery RPC or uncertain state is created.
// 共享页面入场在任何恢复 RPC 或不确定状态创建之前拒绝竞争动作。
it("respects the same synchronous parent mutation gate", async () => {
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click()); acquire.mockReturnValue(false);
  await act(async () => button("skills.plugins.confirm").click());
  expect(recoverInstall).not.toHaveBeenCalled(); expect(release).not.toHaveBeenCalled();
  expect(host.textContent).not.toContain(label("pluginRecovery.unknown"));
});

// In-flight recovery cannot be retargeted or repeated, and disposing the renderer releases only page admission.
// 进行中恢复不能重新定向或重复，销毁渲染器仅释放页面入场。
it("locks identity during recovery and releases admission after disposal without replay", async () => {
  // finish resolves the same already dispatched recovery without cancelling its service work.
  // finish 结束相同已派发恢复，不取消其服务工作。
  let finish!: () => void;
  recoverInstall.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  await openAndRead(); await act(async () => button("pluginRecovery.recover").click());
  await act(async () => button("skills.plugins.confirm").click());
  expect(host.querySelector<HTMLInputElement>("input[list]")?.disabled).toBe(true);
  expect(button("skills.plugins.confirm").disabled).toBe(true);
  await act(async () => button("skills.plugins.confirm").click()); expect(recoverInstall).toHaveBeenCalledTimes(1);
  await act(async () => root.render(null)); readInstall.mockResolvedValue([]);
  await act(async () => finish()); expect(release).toHaveBeenCalledTimes(1);
  expect(recoverInstall).toHaveBeenCalledExactlyOnceWith("unpublished", original.requestId, false);
});
