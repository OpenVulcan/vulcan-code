import { expect, it } from "vitest";
import { closeWorkspaceContextTab, workspaceContextLastTabClosed } from "./context-tabs";

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
