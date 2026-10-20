// @vitest-environment jsdom
// 使用 jsdom 验证可折叠仓库分组与操作身份。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
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
// section finds one repository group by its stable key rather than by its position.
// section 按稳定键查找仓库分组，而不依赖其位置。
// Parameters: repositoryKey is the project-relative key. Returns: the group element, if rendered.
// 参数：repositoryKey 为项目相对键。返回值：已渲染的分组元素。
function section(repositoryKey: string): HTMLElement | null {
  return host.querySelector<HTMLElement>(`.vk-stools-sec[data-kind=review][data-repository-key="${repositoryKey}"]`);
}
// openManager opens the Git settings dialog, the only place where monitoring is switched.
// openManager 打开 Git 设置弹窗，这是切换监听的唯一入口。
// Returns: completion after the dialog renders.
// 返回值：弹窗渲染完成后的结束信号。
async function openManager() {
  await act(async () => host.querySelector<HTMLButtonElement>(".vk-stools-manage")!.click());
}
// managerCheckbox finds one repository checkbox in the Git settings dialog by its visible path.
// managerCheckbox 按可见路径在 Git 设置弹窗中查找仓库复选框。
// Parameters: repositoryKey is the project-relative key. Returns: the checkbox, if rendered.
// 参数：repositoryKey 为项目相对键。返回值：已渲染的复选框。
function managerCheckbox(repositoryKey: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLElement>(".vk-repo-row")]
    .find((row) => repositoryKey === "." ? !row.querySelector("small") : row.querySelector("small")?.textContent === repositoryKey)
    ?.querySelector<HTMLButtonElement>('[role="checkbox"]') ?? undefined;
}
describe("foldable multi-repository Git panel", () => {
  it("renders every monitored repository as a group and routes actions by stable key", async () => {
    // select, open and review observe exact action ownership without a real repository mutation.
    // select、open 与 review 观察精确动作归属，不修改真实仓库。
    const select = vi.fn(); const open = vi.fn(); const review = vi.fn();
    await renderPanel({ onSelectReviewRepository: select, onCommitDialogOpenChange: open, onOpenReviewChanges: review });
    expect(host.querySelectorAll(".vk-stools-sec[data-kind=review]")).toHaveLength(2);
    expect(host.textContent).not.toContain("监听中");
    // Only the first group starts expanded; the folded one keeps its line delta on the title row.
    // 仅首个分组默认展开；折叠分组在标题行保留行数增减。
    expect(section(".")!.querySelector("[data-row=commit]")).not.toBeNull();
    expect(section("packages/repo")!.querySelector("[data-row=commit]")).toBeNull();
    expect(section("packages/repo")!.querySelector(".vk-stools-status")?.textContent).toBe("+3-1");
    await act(async () => section("packages/repo")!.querySelector<HTMLButtonElement>("[data-action=fold]")!.click());
    expect(section("packages/repo")!.querySelector(".vk-stools-status")?.textContent).toBe("");
    await act(async () => section("packages/repo")!.querySelector<HTMLButtonElement>("[data-row=changes]")!.click());
    expect(review).toHaveBeenLastCalledWith("packages/repo");
    await act(async () => section("packages/repo")!.querySelector<HTMLButtonElement>("[data-row=commit]")!.click());
    expect(select).toHaveBeenLastCalledWith("packages/repo");
    expect(open).toHaveBeenLastCalledWith(true);
    // Folding the first group hides its body without affecting the other.
    // 折叠首个分组只隐藏其正文，不影响另一分组。
    await act(async () => section(".")!.querySelector<HTMLButtonElement>("[data-action=fold]")!.click());
    expect(section(".")!.querySelector(".vk-stools-list")).toBeNull();
    expect(section("packages/repo")!.querySelector(".vk-stools-list")).not.toBeNull();
  });
  it("marks only the controlled active repository while its dialog is open", async () => {
    await renderPanel({ activeReviewRepositoryKey: ".", commitDialogOpen: true });
    expect(section(".")!.querySelector("[data-row=commit]")?.getAttribute("aria-expanded")).toBe("true");
    await act(async () => section("packages/repo")!.querySelector<HTMLButtonElement>("[data-action=fold]")!.click());
    expect(section("packages/repo")!.querySelector("[data-row=commit]")?.getAttribute("aria-expanded")).toBe("false");
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
    expect(host.querySelector("[data-row=commit]")).toBeNull();
  });
  it("keeps unobserved and failed snapshots explicit without enabling stale commit actions", async () => {
    await renderPanel({ reviewRepositories: [{ ...repositories[0], summary: null, loading: true }] });
    expect(host.textContent).toContain("正在读取仓库状态");
    expect(host.querySelector("[data-row=commit]")).toBeNull();
    await renderPanel({ reviewRepositories: [{ ...repositories[0], error: "repository read failed" }] });
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("repository read failed");
    expect(host.querySelector<HTMLButtonElement>("[data-row=commit]")!.disabled).toBe(true);
  });
  it("switches monitoring only from Git settings, and unmonitored repositories disappear from the panel", async () => {
    // toggle records one explicit monitoring action from the settings dialog.
    // toggle 记录来自设置弹窗的一次显式监听动作。
    const toggle = vi.fn().mockResolvedValue(undefined);
    await renderPanel({ onToggleReviewMonitoring: toggle });
    await openManager();
    expect(document.querySelectorAll(".vk-repo-row")).toHaveLength(2);
    await act(async () => managerCheckbox("packages/repo")!.click());
    expect(toggle).toHaveBeenLastCalledWith("packages/repo", false);
    await renderPanel({ reviewRepositories: [repositories[0], { ...repositories[1], monitoringEnabled: false, summary: null }], onToggleReviewMonitoring: toggle });
    expect(section("packages/repo")).toBeNull();
    // A single remaining repository no longer folds.
    // 仅剩一个仓库时不再折叠。
    expect(host.querySelector("[data-action=fold]")).toBeNull();
    await renderPanel({ reviewRepositories: repositories.map((repository) => ({ ...repository, monitoringEnabled: false, summary: null })), onToggleReviewMonitoring: toggle });
    expect(host.querySelector(".vk-stools-sec[data-kind=review]")).toBeNull();
    await act(async () => managerCheckbox(".")!.click());
    expect(toggle).toHaveBeenLastCalledWith(".", true);
  });
  it("disables only the busy repository while another repository remains actionable", async () => {
    // busy resolves one operation by exact key, as the workspace host does.
    // busy 按精确键解析一个操作，与工作区宿主一致。
    const busy = (key: string) => key === "." ? { label: "正在提交" } : null;
    await renderPanel({ activeReviewRepositoryKey: ".", commitBusyStateForRepository: busy, commitBusyState: { label: "正在提交" } });
    expect(section(".")!.querySelector<HTMLButtonElement>("[data-row=commit]")!.disabled).toBe(true);
    await act(async () => section("packages/repo")!.querySelector<HTMLButtonElement>("[data-action=fold]")!.click());
    expect(section("packages/repo")!.querySelector<HTMLButtonElement>("[data-row=commit]")!.disabled).toBe(false);
    // A folded busy group still shows its progress on the title row.
    // 折叠的忙碌分组仍在标题行显示进度。
    await act(async () => section(".")!.querySelector<HTMLButtonElement>("[data-action=fold]")!.click());
    expect(section(".")!.querySelector('.vk-stools-status [aria-label="正在提交"]')).not.toBeNull();
  });
  it("does not let an old workspace completion clear a new workspace monitoring request", async () => {
    // finishOld and finishNew settle two different workspace requests that intentionally share the root repo key.
    // finishOld 与 finishNew 完成两个刻意共享根仓库键的不同工作区请求。
    let finishOld!: () => void; let finishNew!: () => void;
    const toggle = vi.fn().mockReturnValueOnce(new Promise<void>((resolve) => { finishOld = resolve; }))
      .mockReturnValueOnce(new Promise<void>((resolve) => { finishNew = resolve; }));
    await renderPanel({ reviewRepositoryOrderStorageKey: "workspace-a", onToggleReviewMonitoring: toggle });
    await openManager();
    await act(async () => managerCheckbox(".")!.click());
    await renderPanel({ reviewRepositoryOrderStorageKey: "workspace-b", onToggleReviewMonitoring: toggle });
    expect(managerCheckbox(".")!.disabled).toBe(false);
    await act(async () => managerCheckbox(".")!.click());
    await act(async () => finishOld());
    expect(managerCheckbox(".")!.disabled).toBe(true);
    await act(async () => finishNew());
    expect(managerCheckbox(".")!.disabled).toBe(false);
  });
  it("keeps a clean single repository inspectable without offering an empty commit", async () => {
    await renderPanel({ reviewRepositories: [{ ...repositories[0], summary: { ...repositories[0].summary!, changedFiles: 0,
      additions: 0, deletions: 0, unstagedChangedFiles: 0, unstagedAdditions: 0, unstagedDeletions: 0 } }] });
    expect(host.querySelector("[data-action=fold]")).toBeNull();
    expect(host.querySelector(".vk-stools-manage")).toBeNull();
    expect(host.querySelector<HTMLButtonElement>("[data-row=changes]")!.disabled).toBe(false);
    expect(host.querySelector<HTMLButtonElement>("[data-row=commit]")!.disabled).toBe(true);
  });
});
