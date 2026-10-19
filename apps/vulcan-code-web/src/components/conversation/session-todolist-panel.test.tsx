import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { ConversationSessionTodolistPanel, type ConversationSessionReviewSummary } from "./session-todolist-panel";

// PanelProps identifies the real panel input contract used by the layout scenarios.
// PanelProps 标识布局场景使用的真实面板输入契约。
type PanelProps = React.ComponentProps<typeof ConversationSessionTodolistPanel>;
// subagentNavigation contains a closed task retained for user inspection during the current turn.
// subagentNavigation 包含当前回合保留以供用户查看的已关闭任务。
const subagentNavigation: PanelProps["subagentNavigation"] = {
  parentConversationId: "parent", activeConversationId: "parent",
  tasks: [{ taskId: "child", publicTaskId: "child", label: "测试智能体", runState: "closed", isRunning: false, isActive: false }],
};
// reviewSummary represents real Git content, even when the repository has no changes.
// reviewSummary 代表真实 Git 内容，即使仓库没有变更。
const reviewSummary: ConversationSessionReviewSummary = {
  branchName: "main", branchNames: ["main"], unpushedCommits: 0,
  additions: 0, deletions: 0, changedFiles: 0, stagedChangedFiles: 0, stagedAdditions: 0,
  stagedDeletions: 0, unstagedChangedFiles: 0, unstagedAdditions: 0, unstagedDeletions: 0,
};

// renderPanel renders the production panel without backend actions or persisted preferences.
// renderPanel 渲染生产面板，不执行后端操作或持久化偏好。
// Parameters: overrides supplies each scenario's visible resources. Returns: static panel markup.
// 参数：overrides 提供各场景的可见资源。返回值：静态面板标记。
function renderPanel(overrides: Partial<PanelProps>): string {
  return renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <ConversationSessionTodolistPanel expanded todolist={null} reviewSummary={null} terminalSessions={[]}
      subagentNavigation={null} commitDialogOpen={false} commitBusyState={null}
      onExpandedChange={() => undefined} onCommitDialogOpenChange={() => undefined}
      onOpenReviewChanges={() => undefined} onStopCommandSession={async () => undefined}
      onOpenSubagentPanel={async () => true} {...overrides} />
  </TooltipProvider></I18nProvider>);
}

describe("session tool panel header layout", () => {
  it.each<Partial<PanelProps>>([
    { subagentNavigation },
    { terminalSessions: [{ id: "terminal", command: "pwsh", status: "running", preview: "" }] },
    { todolist: { items: [{ step: "检查布局", status: "pending" }], completedCount: 0, totalCount: 1 } },
    { subagentNavigation, terminalSessions: [{ id: "terminal", command: "pwsh", status: "running", preview: "" }] },
  ])("shares the first resource title row when Git content is absent: %j", (resources) => {
    // markup must retain a single collapse control without reserving the Git settings row.
    // markup 必须保留唯一收起控件，不预留 Git 设置行。
    const markup = renderPanel(resources);
    expect(markup).toContain("conversation-session-tool-panel-header-inline");
    expect(markup.match(/aria-label="折叠会话工具"/g)).toHaveLength(1);
    expect(markup).not.toContain("conversation-session-review-section");
  });

  it("lets a single Git summary share its title row with the collapse control", () => {
    // markup retains Git layout when zero changes still constitutes a present repository, without an empty top row.
    // markup 在零变更仍代表仓库存在时保留 Git 布局，且不留空白顶部行。
    const markup = renderPanel({ reviewSummary, subagentNavigation });
    expect(markup).toContain("conversation-session-tool-panel-header-inline");
    expect(markup).not.toContain("conversation-session-review-manager-trigger");
    expect(markup.match(/aria-label="折叠会话工具"/g)).toHaveLength(1);
    expect(markup).toContain("conversation-session-review-section");
    expect(markup).toContain("main");
  });

  it("keeps disabled repositories reachable through their settings row", () => {
    // markup must not confuse disabled monitoring with the absence of Git information.
    // markup 不得将关闭监听误认为没有 Git 信息。
    const markup = renderPanel({ subagentNavigation, reviewRepositories: [{
      repositoryKey: ".", repositoryName: "repo", relativePath: ".", isPrimary: true,
      summary: null, monitoringEnabled: false, loading: false, error: null,
    }] });
    expect(markup).not.toContain("conversation-session-tool-panel-header-inline");
    expect(markup).toContain("conversation-session-review-manager-trigger");
  });

  it("renders no panel when every resource is absent", () => {
    expect(renderPanel({})).toBe("");
  });
});
