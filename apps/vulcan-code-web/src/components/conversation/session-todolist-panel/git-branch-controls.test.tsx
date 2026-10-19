// @vitest-environment jsdom
// 使用 jsdom 验证分支切换浮层、新建分支弹窗与 Git 图谱分页。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { WorkspaceGitHistoryCommit } from "@/diff/types";
import {
  ConversationSessionBranchMenu,
  layoutGitGraph,
  type ConversationSessionGitBranchActions,
} from "./git-branch-controls";

// host and root isolate each mounted branch menu.
// host 与 root 隔离每次挂载的分支菜单。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div"); document.body.appendChild(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks();
  document.body.innerHTML = "";
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});

// commit builds one history row with only the fields a scenario cares about.
// commit 构建仅包含场景所需字段的单条历史记录。
function commit(hash: string, parents: string[], overrides: Partial<WorkspaceGitHistoryCommit> = {}): WorkspaceGitHistoryCommit {
  return { hash, shortHash: hash.slice(0, 7), parents, authorName: "Ada", authorEmail: "ada@example.com",
    authorDate: "2026-09-24T19:30:57+08:00", subject: `subject ${hash}`, body: "", refs: [], ...overrides };
}

// fakeActions records calls and serves the given history pages in order.
// fakeActions 记录调用并按顺序返回给定历史页。
function fakeActions(pages: { commits: WorkspaceGitHistoryCommit[]; hasMore: boolean }[] = []) {
  const actions = {
    commitBranchPrefix: "vulcan/",
    loadBranches: vi.fn(async () => ({ currentBranch: "main", branches: ["main", "feature/login"] })),
    switchBranch: vi.fn(async () => undefined),
    loadHistory: vi.fn(async (_key: string | null, skip: number) =>
      pages[pages.findIndex((_, index) => pages.slice(0, index).reduce((sum, page) => sum + page.commits.length, 0) === skip)]
        ?? { commits: [], hasMore: false }),
  } satisfies ConversationSessionGitBranchActions;
  return actions;
}

// renderMenu mounts the branch row for repository `repo` on branch `main`.
// renderMenu 挂载仓库 `repo` 在 `main` 分支上的分支行。
async function renderMenu(actions: ConversationSessionGitBranchActions) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ConversationSessionBranchMenu actions={actions} branchName="main" busy={false} changedFiles={3} repositoryKey="repo" />
  </I18nProvider>));
}

// click dispatches a click and lets async React work settle.
// click 触发点击并等待异步 React 工作完成。
async function click(element: Element | null | undefined) {
  expect(element, "expected element is missing; the rendered structure may have changed").toBeTruthy();
  await act(async () => { (element as HTMLElement).click(); });
}

// buttonByText finds a button whose text contains the given label.
// buttonByText 查找文本包含指定标签的按钮。
function buttonByText(label: string) {
  return Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.includes(label));
}

describe("layoutGitGraph", () => {
  it("keeps a linear history on one lane", () => {
    const layout = layoutGitGraph([commit("c", ["b"]), commit("b", ["a"]), commit("a", [])]);
    expect(layout.laneCount).toBe(1);
    expect(layout.rows.map((row) => row.lane)).toEqual([0, 0, 0]);
    expect(layout.rows[0].incoming).toEqual([]);
    expect(layout.rows[1].incoming).toEqual([0]);
    expect(layout.rows[2].outgoing).toEqual([]);
  });

  it("opens a second lane for a merge and folds it back onto the mainline at the fork point", () => {
    const layout = layoutGitGraph([
      commit("m", ["a", "b"]),
      commit("b", ["base"]),
      commit("a", ["base"]),
      commit("base", []),
    ]);
    expect(layout.laneCount).toBe(2);
    expect(layout.rows[0].outgoing).toEqual([0, 1]);
    expect(layout.rows[1]).toMatchObject({ lane: 1, passThrough: [{ from: 0, to: 0 }] });
    expect(layout.rows[2]).toMatchObject({ lane: 0, outgoing: [0], passThrough: [{ from: 1, to: 0 }] });
    expect(layout.rows[3]).toMatchObject({ lane: 0, incoming: [0], passThrough: [] });
  });
});

describe("ConversationSessionBranchMenu", () => {
  it("lists branches, marks the current one, and switches to another branch", async () => {
    const actions = fakeActions();
    await renderMenu(actions);
    await click(host.querySelector("[data-row=branch]"));
    expect(actions.loadBranches).toHaveBeenCalledWith("repo");
    const current = document.querySelector(".vk-branch-pop [aria-selected=true]");
    expect(current?.textContent).toContain("main");
    expect(current?.textContent).toContain("未提交的更改：3 个文件");
    await click(buttonByText("feature/login"));
    expect(actions.switchBranch).toHaveBeenCalledWith("repo", "feature/login", false);
  });

  it("recommends a prefixed branch name and creates it from HEAD", async () => {
    const actions = fakeActions();
    await renderMenu(actions);
    await click(host.querySelector("[data-row=branch]"));
    await click(buttonByText("创建并检出新分支"));
    const input = document.querySelector<HTMLInputElement>("[data-form=create-branch] input");
    expect(input?.value).toBe("vulcan/main");
    await click(buttonByText("创建并切换"));
    expect(actions.switchBranch).toHaveBeenCalledWith("repo", "vulcan/main", true);
  });

  it("toggles commit details and loads the next page when scrolled to the bottom", async () => {
    const firstPage = Array.from({ length: 30 }, (_, index) => commit(`h${String(index).padStart(2, "0")}`, [`h${String(index + 1).padStart(2, "0")}`]));
    firstPage[0].refs = [{ name: "main", kind: "branch", head: true }];
    const actions = fakeActions([
      { commits: firstPage, hasMore: true },
      { commits: [commit("h30", [])], hasMore: false },
    ]);
    // Scroll metrics make the first page overflow so only an explicit scroll requests page two.
    // 滚动尺寸让首页溢出，因此只有显式滚动才会请求第二页。
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(1000);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(400);
    await renderMenu(actions);
    await click(host.querySelector("[data-row=branch]"));
    await click(buttonByText("Git 图谱"));
    expect(actions.loadHistory).toHaveBeenCalledTimes(1);
    expect(actions.loadHistory).toHaveBeenLastCalledWith("repo", 0);
    const rows = () => document.querySelectorAll(".vk-graph-body .vk-graph-row");
    expect(rows()).toHaveLength(30);
    expect(rows()[0].textContent).toContain("HEAD");

    await click(rows()[1]);
    expect(document.querySelector(".vk-graph-detail")?.textContent).toContain("h01");
    await click(rows()[1]);
    expect(document.querySelector(".vk-graph-detail")).toBeNull();

    const body = document.querySelector<HTMLDivElement>(".vk-graph-body")!;
    await act(async () => {
      body.scrollTop = 600;
      body.dispatchEvent(new Event("scroll"));
    });
    expect(actions.loadHistory).toHaveBeenLastCalledWith("repo", 30);
    expect(rows()).toHaveLength(31);
    expect(document.querySelector(".vk-graph-body")?.textContent).toContain("已显示全部提交");
  });
});
