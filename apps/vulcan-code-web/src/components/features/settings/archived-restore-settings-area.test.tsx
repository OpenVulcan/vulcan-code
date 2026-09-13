// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ArchivedRestoreSettingsArea } from "./misc-settings-areas";
import type { SettingsActionProps } from "./settings-action-types";

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
  expect(host.textContent).toContain("选择已保留");
  expect(host.querySelector('[aria-label="选择会话 测试会话"]')?.getAttribute("aria-checked")).toBe("true");
  restore.mockResolvedValueOnce(undefined);
  await act(async () => restoreButton.click());
  expect(restore).toHaveBeenCalledTimes(2);
  expect(host.querySelector('[aria-label="选择会话 测试会话"]')?.getAttribute("aria-checked")).toBe("false");
  expect(remove).not.toHaveBeenCalled();
  expect(host.textContent).toContain("测试项目");
  expect(host.textContent).not.toContain("WebUI snapshot");
});
