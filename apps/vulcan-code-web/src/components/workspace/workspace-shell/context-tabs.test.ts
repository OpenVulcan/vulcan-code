import { expect, it } from "vitest";
import {
  closeWorkspaceContextTab,
  emptyWorkspaceContextTabState,
  openWorkspaceViewerContextTab,
  workspaceContextLastTabClosed,
} from "./context-tabs";

// The viewer tab is a single tab that switches to each newly opened file and is named after it.
// 查看页签是单个页签，每次切换到新打开的文件并以其命名。
it("reuses one viewer tab for each opened file", () => {
  const t = (key: string) => key;
  const first = openWorkspaceViewerContextTab(emptyWorkspaceContextTabState(), "D:\\docs\\report.pdf", t);
  const second = openWorkspaceViewerContextTab(first, "/home/u/budget.xlsx", t);
  const viewers = second.tabs.filter((tab) => tab.kind === "viewer");
  expect(viewers).toHaveLength(1);
  expect(second.activeTabId).toBe(viewers[0].id);
  expect(viewers[0]).toMatchObject({ title: "budget.xlsx", viewerPath: "/home/u/budget.xlsx" });
  expect(first.tabs.find((tab) => tab.kind === "viewer")).toMatchObject({ title: "report.pdf", viewerPath: "D:\\docs\\report.pdf" });
});

// Closing the last page in the same conversation is the only transition that closes the panel.
// 只有在同一会话内关闭最后一个页面的转换才会关闭面板。
it("reports a last-tab close only within the same conversation", () => {
  const closed = closeWorkspaceContextTab(
    { activeTabId: "review", tabs: [{ createdAt: 1, id: "review", kind: "review", selectedFileName: null, title: "review" }] },
    "review",
  );
  expect(closed.tabs).toHaveLength(0);
  expect(workspaceContextLastTabClosed({ cacheKey: "a", count: 1 }, { cacheKey: "a", count: closed.tabs.length })).toBe(true);
  expect(workspaceContextLastTabClosed({ cacheKey: "a", count: 2 }, { cacheKey: "a", count: 1 })).toBe(false);
  expect(workspaceContextLastTabClosed({ cacheKey: "a", count: 0 }, { cacheKey: "a", count: 0 })).toBe(false);
  expect(workspaceContextLastTabClosed({ cacheKey: "a", count: 1 }, { cacheKey: "b", count: 0 })).toBe(false);
});
