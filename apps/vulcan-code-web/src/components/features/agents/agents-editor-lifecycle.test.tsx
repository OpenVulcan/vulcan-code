// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AgentsArea } from "./agents-area";
import type { AgentActionProps } from "./agent-area-types";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { TooltipProvider } from "@/components/ui/tooltip";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key);
  return { ...actual, useI18n: () => ({ locale: "zh-CN", setLocale: () => undefined, t }) };
});

// The component mount and action stubs own no real service or user definition files.
// 组件挂载与动作桩不持有真实服务或用户定义文件。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// mountEditable renders a single user definition using the maintained exact workbench fixture.
// mountEditable 使用持续维护的准确工作台夹具渲染一个用户定义。
// Parameters: callbacks control synthetic outcomes; systemBacked selects immutable built-in metadata. Returns: mounted definition id.
// 参数：回调控制合成结果；systemBacked 选择不可修改的内置元数据。返回值：已挂载定义的标识。
async function mountEditable(save: AgentActionProps["onSaveAgentDefinition"], remove: AgentActionProps["onDeleteAgentDefinition"], restore?: AgentActionProps["onRestoreAgentDefinitionDefault"], systemBacked = false) {
  // workbench is detached fallback data, never a persisted workspace snapshot.
  // workbench 是独立回退数据，不是持久化工作区快照。
  const workbench = browserFallbackSnapshot();
  // definition preserves every required backend field while selecting an editable user scope.
  // definition 保留全部必需后端字段，同时选择可编辑用户作用域。
  const definition = { ...workbench.subagentCatalog.definitions[0], agentId: "audit-editable", name: "验收智能体", scopeKey: systemBacked ? "system" as const : "user" as const, scope: systemBacked ? "System" : "User", canEdit: true, canDelete: !systemBacked, overrideOf: null, canRestoreDefault: Boolean(restore) };
  workbench.subagentCatalog.definitions = [definition];
  // actions are complete page callbacks; unrelated controls remain deterministic no-ops.
  // actions 是完整页面回调；不相关控件保持确定性的空操作。
  const actions: AgentActionProps = { onSaveAgentDefinition: save, onDeleteAgentDefinition: remove, onRefreshAgentCatalog: async () => {}, onOpenAgentUserDirectory: async () => {}, onSetAgentDefinitionEnabled: async () => {}, onRestoreAgentDefinitionDefault: restore ?? (async () => {}) };
  await act(async () => root.render(<TooltipProvider><AgentsArea actions={actions} workbench={workbench} /></TooltipProvider>));
  return definition.agentId;
}

it("keeps the editor after rejection, rejects rapid duplicate saves, and permits retry", async () => {
  // rejectSave controls only the accepted synthetic request.
  // rejectSave 只控制已接受的合成请求。
  let rejectSave: (error: Error) => void = () => { throw new Error("Save was not started"); };
  // save is intentionally pending until the failure branch is exercised.
  // save 刻意等待，直到触发失败分支。
  const save = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectSave = reject; }));
  await mountEditable(save, vi.fn());
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="编辑"]')!.click());
  // form and submit operate the real editor rather than reproducing its guard implementation.
  // form 与 submit 操作真实编辑器，而非复刻其守卫实现。
  const form = document.querySelector<HTMLFormElement>(".agent-editor-form")!;
  await act(async () => { form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); });
  expect(save).toHaveBeenCalledTimes(1);
  await act(async () => rejectSave(new Error("Synthetic save failure")));
  expect(document.querySelector(".agent-editor-form")).toBe(form);
  expect(form.textContent).toContain("保存失败，草稿已保留");
  save.mockResolvedValueOnce(undefined);
  await act(async () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(save).toHaveBeenCalledTimes(2);
  expect(document.querySelector(".agent-editor-form")).toBeNull();
});

it("folds built-in metadata while retaining readable full prompts and editable overrides", async () => {
  // save records only explicit submission; disclosure controls must never persist a draft.
  // save 只记录明确提交；展开收起控件绝不能持久化草稿。
  const save = vi.fn();
  await mountEditable(save, vi.fn(), undefined, true);
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="编辑"]')!.click());
  // definitionDetails contains only the source metadata, leaving append and policy controls outside.
  // definitionDetails 仅包含源元数据，追加提示词与策略控件位于其外。
  const definitionDetails = document.querySelector<HTMLDetailsElement>(".agent-editor-definition")!;
  expect(definitionDetails.open).toBe(false);
  expect(definitionDetails.querySelector<HTMLTextAreaElement>("textarea")!.readOnly).toBe(true);
  expect(definitionDetails.querySelector<HTMLTextAreaElement>("textarea")!.disabled).toBe(false);
  // append remains editable and mounted as the source definition is opened and closed.
  // 源定义展开和收起时，append 保持可编辑且不卸载。
  const append = document.querySelector<HTMLTextAreaElement>(".agent-prompt-field:not(.primary) textarea")!;
  expect(append.readOnly).toBe(false);
  expect(definitionDetails.contains(append)).toBe(false);
  await act(async () => definitionDetails.querySelector("summary")!.click());
  expect(definitionDetails.open).toBe(true);
  await act(async () => definitionDetails.querySelector("summary")!.click());
  expect(definitionDetails.open).toBe(false);
  expect(document.querySelector(".agent-prompt-field:not(.primary) textarea")).toBe(append);
  expect(save).not.toHaveBeenCalled();
});

it("requires deletion confirmation and leaves the definition untouched when cancelled", async () => {
  // remove records accepted deletions without changing any actual file.
  // remove 记录已接受的删除动作，不修改任何实际文件。
  const remove = vi.fn(async () => {});
  const agentId = await mountEditable(vi.fn(), remove);
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="删除"]')!.click());
  expect(remove).not.toHaveBeenCalled();
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')].find((button) => button.textContent === "取消")!.click());
  expect(remove).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="删除"]')!.click());
  await act(async () => [...document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')].find((button) => button.textContent === "删除")!.click());
  expect(remove).toHaveBeenCalledExactlyOnceWith(agentId);
});

it("keeps failed deletion open for retry and exposes the declared restore action", async () => {
  // remove fails once so the real confirmation's target and pending guards can be inspected.
  // remove 首次失败，用于检查真实确认框的目标和等待守卫。
  const remove = vi.fn().mockRejectedValueOnce(new Error("合成删除失败")).mockResolvedValue(undefined);
  // restore records only the declared exact agent id after explicit confirmation.
  // restore 仅在明确确认后记录声明的准确智能体标识。
  const restore = vi.fn(async () => {});
  const agentId = await mountEditable(vi.fn(), remove, restore);
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="删除"]')!.click());
  // confirmationAction returns the current destructive action without matching the background row.
  // confirmationAction 返回当前破坏性操作，不匹配背景目录行。
  const confirmationAction = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')].find((button) => button.textContent === label)!;
  await act(async () => { confirmationAction("删除").click(); confirmationAction("删除").click(); });
  expect(remove).toHaveBeenCalledTimes(1);
  expect(document.querySelector('[role="alertdialog"] [role="alert"]')?.textContent).toBe("合成删除失败");
  await act(async () => confirmationAction("删除").click());
  expect(remove).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="恢复默认"]')!.click());
  expect(restore).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("恢复系统定义");
  await act(async () => confirmationAction("恢复默认").click());
  expect(restore).toHaveBeenCalledExactlyOnceWith(agentId);
});
