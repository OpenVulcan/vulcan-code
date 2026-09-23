// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ArchivedRestoreSettingsArea } from "./misc-settings-areas";
import type { SettingsActionProps } from "./settings-action-types";
import type { ConversationSummary } from "@/types";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));

// The mount owns only synthetic archive metadata; lifecycle callbacks never reach a service.
// 挂载仅持有合成归档元数据；生命周期回调不会访问服务。
let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// Build local archive identities for selection tests; count controls size and no service is contacted.
// 构建用于选择测试的本地归档标识；count 控制数量，不访问服务。
function archiveFixtures(count: number): ConversationSummary[] {
  return Array.from({ length: count }, (_, index) => ({ id: `archive-${index + 1}`, title: `会话${index + 1}`, status: "completed" }));
}

// Row operations must never consume an unrelated batch selection, and deletion requires confirmation.
// 单行操作不得带入其他批量选择，删除必须经过确认。
it("isolates row restore and confirmed deletion from selected conversations", async () => {
  // Observe only exact identities passed to the synthetic lifecycle callbacks.
  // 仅观察传入合成生命周期回调的精确标识。
  const restore = vi.fn(async () => undefined);
  const remove = vi.fn(async () => undefined);
  await act(async () => root.render(<ArchivedRestoreSettingsArea
    actions={{ onRestoreArchivedConversations: restore, onDeleteArchivedConversations: remove } as unknown as SettingsActionProps}
    conversations={archiveFixtures(2)} projects={[]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="选择会话 会话1"]')!.click());
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="恢复会话 会话2"]')!.click());
  expect(restore).toHaveBeenCalledExactlyOnceWith(["archive-2"]);
  expect(host.querySelector('[aria-label="选择会话 会话1"]')?.getAttribute("aria-checked")).toBe("true");
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="删除会话 会话2"]')!.click());
  expect(remove).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"] ul')?.textContent).toBe("会话2");
  // Find and cancel the actual confirmation action, preserving both archive and checkbox selection.
  // 查找并取消真实确认操作，保留归档及复选框选择。
  const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "取消")!;
  await act(async () => cancel.click());
  expect(remove).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="删除会话 会话2"]')!.click());
  // Only explicit confirmation admits the previously displayed row identity.
  // 仅明确确认后才允许操作之前展示的单行标识。
  const confirm = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "彻底删除")!;
  await act(async () => confirm.click());
  expect(remove).toHaveBeenCalledExactlyOnceWith(["archive-2"]);
  expect(host.querySelector('[aria-label="选择会话 会话1"]')?.getAttribute("aria-checked")).toBe("true");
});

// Select-all spans every page and batch restore receives exactly that cross-page set.
// 全选覆盖所有分页，批量恢复接收精确的跨页集合。
it("selects all pages and restores the entire selected batch", async () => {
  // Twenty-one fixtures cross the component's twenty-row page boundary.
  // 二十一项夹具跨过组件每页二十行的分页边界。
  const conversations = archiveFixtures(21);
  const restore = vi.fn(async () => undefined);
  await act(async () => root.render(<ArchivedRestoreSettingsArea
    actions={{ onRestoreArchivedConversations: restore, onDeleteArchivedConversations: vi.fn() } as unknown as SettingsActionProps}
    conversations={conversations} projects={[]} />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="全选归档会话"]')!.click());
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="下一页"]')!.click());
  expect(host.querySelector('[aria-label="选择会话 会话21"]')?.getAttribute("aria-checked")).toBe("true");
  // The batch action is independent of which page is currently visible.
  // 批量操作独立于当前可见分页。
  const batchRestore = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("批量恢复"))!;
  await act(async () => batchRestore.click());
  expect(restore).toHaveBeenCalledExactlyOnceWith(conversations.map((conversation) => conversation.id));
});

// A rejected restore must preserve selected identities, release the guard, and allow an explicit retry.
// 恢复被拒绝时必须保留选择标识、释放守卫，并允许明确重试。
it("deduplicates rapid restores and permits retry after rejection", async () => {
  // rejectRestore controls one pending service response without a real archived conversation.
  // rejectRestore 控制一条等待中的服务响应，不涉及真实归档会话。
  let rejectRestore: (error: Error) => void = () => { throw new Error("Restore was not started"); };
  // restore and remove are the only lifecycle callbacks used by this component.
  // restore 和 remove 是本组件使用的两个生命周期回调。
  const restore = vi.fn(() => new Promise<void>((_resolve, reject) => { rejectRestore = reject; }));
  const remove = vi.fn();
  await act(async () => root.render(<ArchivedRestoreSettingsArea
    actions={{ onRestoreArchivedConversations: restore, onDeleteArchivedConversations: remove } as unknown as SettingsActionProps}
    conversations={[{ id: "archive-1", title: "测试会话", status: "completed", projectId: "project-1" }]}
    projects={[{ id: "project-1", name: "测试项目", path: "test-project" }]}
  />));
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="选择会话 测试会话"]')!.click());
  // The real button receives two synchronous clicks before React can render its disabled state.
  // 在 React 渲染禁用状态前，对真实按钮发送两次同步点击。
  const restoreButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("批量恢复"))!;
  await act(async () => { restoreButton.click(); restoreButton.click(); });
  expect(restore).toHaveBeenCalledTimes(1);
  expect(restore).toHaveBeenLastCalledWith(["archive-1"]);
  await act(async () => rejectRestore(new Error("Synthetic restore failure")));
  expect(host.textContent).toContain("已选中的会话保持不变");
  expect(host.querySelector('[aria-label="选择会话 测试会话"]')?.getAttribute("aria-checked")).toBe("true");
  restore.mockResolvedValueOnce(undefined);
  await act(async () => restoreButton.click());
  expect(restore).toHaveBeenCalledTimes(2);
  expect(host.querySelector('[aria-label="选择会话 测试会话"]')?.getAttribute("aria-checked")).toBe("false");
  expect(remove).not.toHaveBeenCalled();
  expect(host.textContent).toContain("测试项目");
  expect(host.textContent).not.toContain("WebUI snapshot");
});
