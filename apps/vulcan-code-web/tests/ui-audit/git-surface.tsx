import React from "react";
import { I18nProvider } from "../../src/i18n";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { ConversationSessionTodolistPanel, type ConversationSessionReviewRepository } from "../../src/components/conversation/session-todolist-panel";
import { buildWorkspaceProjectGitSummary } from "../../src/components/workspace/workspace-shell/git-summary";
import { auditGitCatalog, auditGitSnapshot } from "./git-fixtures";

// projectRoot belongs only to the isolated fixture and is never opened on the host.
// projectRoot 仅属于隔离夹具，绝不在宿主中打开。
const projectRoot = "X:/ui-audit/project";
// repositories project the same typed read fixtures through the production summary builder.
// repositories 使用生产摘要构造器投影同一批强类型读取夹具。
const repositories: ConversationSessionReviewRepository[] = auditGitCatalog(projectRoot, "long-list").repositories.map(repository => {
  // summary and error preserve each declared read outcome, including a failed peer and a disabled peer.
  // summary 与 error 保留各自声明的读取结果，包括失败与停用的相邻仓库。
  let summary = null;
  let error = null;
  if (repository.monitoringEnabled) {
    try { summary = buildWorkspaceProjectGitSummary(auditGitSnapshot(projectRoot, repository.repositoryKey, "unstaged"), auditGitSnapshot(projectRoot, repository.repositoryKey, "staged")); }
    catch (readError) { error = (readError as Error).message; }
  }
  return { repositoryKey: repository.repositoryKey, repositoryName: repository.name, relativePath: repository.relativePath,
    isPrimary: repository.isPrimary, monitoringEnabled: repository.monitoringEnabled, loading: false, error, summary };
});

/** Mounts the actual Git panel with isolated read outcomes for theme and narrow-layout inspection.
 * 挂载真实 Git 面板，使用隔离读取结果进行主题和窄布局检查。
 * No parameters; returns the unchanged production panel with local expansion and explicit write rejection.
 * 无参数；返回未改动的生产面板，仅展开状态保存在本地，写入明确拒绝。 */
export function AuditGitSurface() {
  // expanded controls only the existing compact/expanded presentation.
  // expanded 仅控制既有紧凑或展开显示。
  const [expanded, setExpanded] = React.useState(true);
  // message records an unsupported action without claiming a successful mutation.
  // message 记录未支持动作，不声称变更成功。
  const [message, setMessage] = React.useState("");
  return <I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <main className="chat-grid" style={{ position: "relative", minHeight: "100vh", padding: 16, background: "var(--background)", color: "var(--foreground)" }}>
      <p>隔离 Git 面板：合成数据</p>
      {message ? <p role="status">{message}</p> : null}
      <ConversationSessionTodolistPanel expanded={expanded} todolist={null} reviewSummary={null} reviewRepositories={repositories}
        terminalSessions={[]} subagentNavigation={null} commitDialogOpen={false} commitBusyState={null}
        onExpandedChange={setExpanded} onCommitDialogOpenChange={() => setMessage("未执行：隔离页面不支持提交。")}
        onOpenReviewChanges={key => setMessage(`已选择 ${key}；实际差异正文另在真实项目只读验收。`)}
        onToggleReviewMonitoring={async () => { throw new Error("隔离页面不保存监听配置。"); }}
        onStopCommandSession={async () => { throw new Error("隔离页面没有终端。"); }} onOpenSubagentPanel={async () => false} />
    </main>
  </TooltipProvider></I18nProvider>;
}
