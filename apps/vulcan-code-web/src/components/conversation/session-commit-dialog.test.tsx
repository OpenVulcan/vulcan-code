// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import { ConversationSessionCommitDialog, type ConversationSessionReviewSummary } from "./session-todolist-panel";

// Each test owns one real React root so the portal-mounted dialog renders into the document.
// 每项测试持有一个真实 React 根，使通过 Portal 挂载的弹窗渲染进文档。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

// renderDialog opens the production commit dialog for one repository summary without running Git actions.
// renderDialog 以单个仓库摘要打开生产提交弹窗，不执行任何 Git 动作。
// Parameters: summary is the visible review state. Returns: the push-only and commit buttons.
// 参数：summary 是可见审查状态。返回值：仅推送与提交按钮。
async function renderDialog(summary: Partial<ConversationSessionReviewSummary>) {
  const reviewSummary: ConversationSessionReviewSummary = {
    branchName: "main", branchNames: ["main"], unpushedCommits: 0,
    additions: 0, deletions: 0, changedFiles: 0, stagedChangedFiles: 0, stagedAdditions: 0,
    stagedDeletions: 0, unstagedChangedFiles: 0, unstagedAdditions: 0, unstagedDeletions: 0, ...summary,
  };
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ConversationSessionCommitDialog open onOpenChange={() => undefined} commitBranchPrefix="" conversationId="session"
      busyState={null} reviewSummary={reviewSummary} workspacePath="D:/repo"
      onBusyStateChange={() => undefined} onRefreshReviewSummary={async () => undefined} />
  </I18nProvider>));
  const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>(".conversation-session-commit-action"));
  const byLabel = (label: string) => buttons.find((button) => button.textContent?.replace("Ctrl+↵", "").trim() === label);
  return { push: byLabel("推送"), commit: byLabel("提交") };
}

describe("session commit dialog push availability", () => {
  it("allows pushing existing commits while uncommitted changes remain", async () => {
    // Staged and unstaged edits stay local; the unpushed commits are still independently publishable.
    // 已暂存与未暂存的修改保留在本地；未推送提交仍可独立推送。
    const { push, commit } = await renderDialog({ unpushedCommits: 2, stagedChangedFiles: 1, unstagedChangedFiles: 3 });
    expect(push?.disabled).toBe(false);
    expect(commit?.disabled).toBe(false);
  });

  it("disables push-only when the branch has no unpushed commits", async () => {
    const { push } = await renderDialog({ unpushedCommits: 0, stagedChangedFiles: 1 });
    expect(push?.disabled).toBe(true);
  });
});
