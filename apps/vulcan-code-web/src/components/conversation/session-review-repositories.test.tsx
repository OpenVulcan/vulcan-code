// @vitest-environment jsdom
// 使用 jsdom 验证真实仓库选择与操作身份。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ConversationSessionTodolistPanel, type ConversationSessionReviewRepository } from "./session-todolist-panel";

// PanelProps uses the production callback and repository contract without invoking Git writes.
// PanelProps 使用生产回调与仓库契约，不调用 Git 写操作。
type PanelProps = React.ComponentProps<typeof ConversationSessionTodolistPanel>;
// repositories intentionally share display names while retaining distinct project-relative keys.
// repositories 刻意共享显示名，同时保留不同的项目相对键。
const repositories: ConversationSessionReviewRepository[] = [".", "packages/repo"].map((repositoryKey, index) => ({
  repositoryKey, repositoryName: "repo", relativePath: repositoryKey, isPrimary: index === 0,
  monitoringEnabled: true, loading: false, error: null,
  summary: { branchName: index === 0 ? "main" : "feature", branchNames: [], unpushedCommits: 0,
    changedFiles: index + 1, additions: 3, deletions: 1, stagedChangedFiles: 0, stagedAdditions: 0,
    stagedDeletions: 0, unstagedChangedFiles: index + 1, unstagedAdditions: 3, unstagedDeletions: 1 },
}));
// host and root isolate mounted panel interaction from other tests.
// host 与 root 隔离挂载面板交互，避免影响其他测试。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div"); document.body.appendChild(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove(); vi.restoreAllMocks();
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});
// renderPanel mounts a real panel; overrides supplies only the scenario-specific authority and callbacks.
// renderPanel 挂载真实面板；overrides 仅提供场景专用权威与回调。
// Returns: completion after React effects settle.
// 返回值：React 副作用完成后的结束信号。
async function renderPanel(overrides: Partial<PanelProps> = {}) {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <ConversationSessionTodolistPanel expanded todolist={null} reviewSummary={null} terminalSessions={[]}
      subagentNavigation={null} commitDialogOpen={false} commitBusyState={null} reviewRepositories={repositories}
      onExpandedChange={() => undefined} onCommitDialogOpenChange={() => undefined}
      onOpenReviewChanges={() => undefined} onStopCommandSession={async () => undefined}
      onOpenSubagentPanel={async () => true} {...overrides} />
  </TooltipProvider></I18nProvider>));
}
describe("focused multi-repository Git panel", () => {
  it("switches one operation area using stable keys and survives late reordered summaries", async () => {
    // select and review observe exact action ownership without a real repository mutation.
    // select 与 review 观察精确动作归属，不修改真实仓库。
    const select = vi.fn(); const review = vi.fn();
    await renderPanel({ onSelectReviewRepository: select, onOpenReviewChanges: review });
    expect(host.querySelectorAll(".conversation-session-review-section")).toHaveLength(1);
    await act(async () => host.querySelectorAll<HTMLButtonElement>(".session-review-repository-option")[1].click());
    expect(select).toHaveBeenLastCalledWith("packages/repo");
    expect(host.querySelector(".conversation-session-review-section")?.getAttribute("data-repository-key")).toBe("packages/repo");
    await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-row")!.click());
    expect(review).toHaveBeenLastCalledWith("packages/repo");
    await renderPanel({ reviewRepositories: [...repositories].reverse(), onSelectReviewRepository: select, onOpenReviewChanges: review });
    expect(host.querySelector(".conversation-session-review-section")?.getAttribute("data-repository-key")).toBe("packages/repo");
    expect(host.textContent).toContain("嵌套仓库 · packages/repo");
  });
  it("honors the controlled active repository and freezes switching while its dialog is open", async () => {
    await renderPanel({ activeReviewRepositoryKey: "packages/repo", commitDialogOpen: true });
    expect(host.querySelector(".conversation-session-review-section")?.getAttribute("data-repository-key")).toBe("packages/repo");
    expect([...host.querySelectorAll<HTMLButtonElement>(".session-review-repository-option")].every((button) => button.disabled)).toBe(true);
  });
  it("opens a fresh read for the exact failed repository without enabling a commit", async () => {
    // review observes the recovery route; no backend write or repository mutation is performed.
    // review 观察恢复路由；不执行后端写入或仓库变更。
    const review = vi.fn();
    await renderPanel({ reviewRepositories: [{ ...repositories[1], summary: null, error: "read failed" }], onOpenReviewChanges: review });
    // retry is the visible recovery action, independent of absent or stale summary data.
    // retry 是可见恢复操作，独立于缺失或陈旧的摘要数据。
    const retry = [...host.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent?.includes("打开审查并重试"));
    expect(retry).toBeDefined();
    await act(async () => retry!.click());
    expect(review).toHaveBeenCalledWith("packages/repo");
    expect(host.querySelector(".conversation-session-commit-trigger")).toBeNull();
  });
  it("keeps unobserved and failed snapshots explicit without enabling stale commit actions", async () => {
    await renderPanel({ reviewRepositories: [{ ...repositories[0], summary: null, loading: true }] });
    expect(host.textContent).toContain("正在读取仓库状态");
    expect(host.querySelector(".conversation-session-commit-trigger")).toBeNull();
    await renderPanel({ reviewRepositories: [{ ...repositories[0], error: "repository read failed" }] });
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("repository read failed");
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-commit-trigger")!.disabled).toBe(true);
  });
  it("keeps monitoring independent, and all-disabled repositories reachable only from Git settings", async () => {
    // toggle records one explicit monitoring action, independent from the selected summary row.
    // toggle 记录独立于已选摘要行的一次显式监听动作。
    const toggle = vi.fn().mockResolvedValue(undefined);
    await renderPanel({ activeReviewRepositoryKey: "packages/repo", onToggleReviewMonitoring: toggle });
    await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.click());
    expect(toggle).toHaveBeenCalledWith("packages/repo", false);
    await renderPanel({ reviewRepositories: repositories.map((repository) => ({ ...repository, monitoringEnabled: false, summary: null })), onToggleReviewMonitoring: toggle });
    expect(host.querySelector(".conversation-session-review-section")).toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-manager-trigger")!.click());
    expect(document.querySelectorAll(".conversation-session-review-manager-row")).toHaveLength(2);
    await act(async () => document.querySelector<HTMLInputElement>('.conversation-session-review-manager-check input')!.click());
    expect(toggle).toHaveBeenLastCalledWith(".", true);
  });
  it("disables only the busy repository while another repository remains actionable", async () => {
    // busy resolves one operation by exact key, as the workspace host does.
    // busy 按精确键解析一个操作，与工作区宿主一致。
    const busy = (key: string) => key === "." ? { label: "正在提交" } : null;
    await renderPanel({ activeReviewRepositoryKey: ".", commitBusyStateForRepository: busy });
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-commit-trigger")!.disabled).toBe(true);
    await renderPanel({ activeReviewRepositoryKey: "packages/repo", commitBusyStateForRepository: busy, commitBusyState: { label: "另一仓库正在提交" } });
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-commit-trigger")!.disabled).toBe(false);
  });
  it("does not let an old workspace completion clear a new workspace monitoring request", async () => {
    // finishOld and finishNew settle two different workspace requests that intentionally share the root repo key.
    // finishOld 与 finishNew 完成两个刻意共享根仓库键的不同工作区请求。
    let finishOld!: () => void; let finishNew!: () => void;
    const toggle = vi.fn().mockReturnValueOnce(new Promise<void>((resolve) => { finishOld = resolve; }))
      .mockReturnValueOnce(new Promise<void>((resolve) => { finishNew = resolve; }));
    await renderPanel({ reviewRepositoryOrderStorageKey: "workspace-a", onToggleReviewMonitoring: toggle });
    await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.click());
    await renderPanel({ reviewRepositoryOrderStorageKey: "workspace-b", onToggleReviewMonitoring: toggle });
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.disabled).toBe(false);
    await act(async () => host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.click());
    await act(async () => finishOld());
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.disabled).toBe(true);
    await act(async () => finishNew());
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-monitor-toggle")!.disabled).toBe(false);
  });
  it("keeps a clean single repository inspectable without offering an empty commit", async () => {
    await renderPanel({ reviewRepositories: [{ ...repositories[0], summary: { ...repositories[0].summary!, changedFiles: 0,
      additions: 0, deletions: 0, unstagedChangedFiles: 0, unstagedAdditions: 0, unstagedDeletions: 0 } }] });
    expect(host.querySelector(".session-review-repository-list")).toBeNull();
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-review-row")!.disabled).toBe(false);
    expect(host.querySelector<HTMLButtonElement>(".conversation-session-commit-trigger")!.disabled).toBe(true);
  });
});
