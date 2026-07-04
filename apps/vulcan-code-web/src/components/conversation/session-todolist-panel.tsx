import React from "react";
import {
  Bot,
  Boxes,
  Check,
  CheckCircle2,
  ChevronDown,
  Circle,
  ClipboardList,
  CloudUpload,
  FileDiff,
  GitBranch,
  GitCommitHorizontal,
  ListChecks,
  LoaderCircle,
  Maximize2,
  Minimize2,
  TerminalSquare,
} from "lucide-react";
import {
  terminalSessionViewModel,
  terminalStateClass,
} from "@/components/conversation/conversation-terminal-model";
import { ConversationSessionTerminalSection } from "@/components/conversation/session-todolist-panel/terminal-section";
import { toast } from "sonner";
import {
  generateGitCommitMessage,
  runGitCommitAction,
  type GitCommitActionResponse,
} from "@/service/conversation-actions-service";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { CommandSessionSummary, SessionTodolistItemSummary, SessionTodolistSummary, WorkbenchSnapshot } from "@/types";

// Conversation session tool morph animation duration in milliseconds.
// 会话工具形态切换动画的毫秒时长。
const CONVERSATION_SESSION_TOOL_TRANSITION_MS = 320;

// ConversationSessionToolTransitionPhase describes the active morph animation direction.
// ConversationSessionToolTransitionPhase 描述当前形态切换动画方向。
type ConversationSessionToolTransitionPhase = "collapsing" | "expanding" | null;

// ConversationSessionReviewSummary describes compact Git review state shown beside session progress.
// ConversationSessionReviewSummary 描述与会话进程并列展示的紧凑 Git 审查状态。
export interface ConversationSessionReviewSummary {
  // Current checked-out branch name.
  // 当前检出的分支名称。
  branchName: string | null;
  // Number of current-branch commits that can be pushed without creating a new commit.
  // 当前分支可在不创建新提交时直接推送的提交数量。
  unpushedCommits: number;
  // Known Git branch names from the review snapshot.
  // 审核快照中已知的 Git 分支名称。
  branchNames: string[];
  // Combined uncommitted added line count.
  // 未提交变更合计新增行数。
  additions: number;
  // Combined uncommitted deleted line count.
  // 未提交变更合计删除行数。
  deletions: number;
  // Combined uncommitted changed file count.
  // 未提交变更合计文件数。
  changedFiles: number;
  // Changed file count that is already staged in the Git index.
  // 已经进入 Git 暂存区的变更文件数。
  stagedChangedFiles: number;
  // Added line count that is already staged in the Git index.
  // 已经进入 Git 暂存区的新增行数。
  stagedAdditions: number;
  // Deleted line count that is already staged in the Git index.
  // 已经进入 Git 暂存区的删除行数。
  stagedDeletions: number;
  // Changed file count that still lives in the working tree.
  // 仍位于工作区未暂存状态的变更文件数。
  unstagedChangedFiles: number;
  // Added line count that still lives in the working tree.
  // 仍位于工作区未暂存状态的新增行数。
  unstagedAdditions: number;
  // Deleted line count that still lives in the working tree.
  // 仍位于工作区未暂存状态的删除行数。
  unstagedDeletions: number;
}

// ConversationSessionCommitTarget identifies where the local commit draft would land.
// ConversationSessionCommitTarget 标识本地提交草稿的目标分支位置。
type ConversationSessionCommitTarget = "current" | "new";

// ConversationSessionCommitActionId identifies one runnable action in the commit dialog.
// ConversationSessionCommitActionId 标识提交弹窗内的单个可执行动作。
type ConversationSessionCommitActionId = "commit" | "commitPush" | "push";

// ConversationSessionCommitBusyState describes the Git action phase shown on shared commit triggers.
// ConversationSessionCommitBusyState 描述共享提交入口展示的 Git 动作阶段。
export type ConversationSessionCommitBusyState = {
  // Active phase label shown on the collapsed Git action trigger.
  // 折叠 Git 动作入口展示的当前阶段文案。
  label: string;
} | null;

// ConversationSessionSubagentTask describes one backend-projected subagent task row.
// ConversationSessionSubagentTask 描述一条后端投影出的子代理任务行。
type ConversationSessionSubagentTask = NonNullable<NonNullable<WorkbenchSnapshot["subagentNavigation"]>["tasks"]>[number];

// ConversationSessionTodolistPanel renders the session todolist inside the main conversation surface.
// ConversationSessionTodolistPanel 在主会话界面内渲染当前会话的 todolist。
export function ConversationSessionTodolistPanel(props: {
  // Current todolist summary projected from the selected session runtime state.
  // 从当前选中会话运行时状态投影出的 todolist摘要。
  todolist: SessionTodolistSummary | null;
  // Current Git review summary projected from the selected workspace.
  // 从当前选中工作区投影出的 Git 审查摘要。
  reviewSummary: ConversationSessionReviewSummary | null;
  // Whether the shared Git commit dialog is currently open.
  // 共享 Git 提交弹窗当前是否打开。
  commitDialogOpen: boolean;
  // Current shared Git action phase shown by every commit trigger.
  // 每个提交入口展示的当前共享 Git 动作阶段。
  commitBusyState: ConversationSessionCommitBusyState;
  // Current terminal sessions owned by the selected conversation.
  // 当前选中会话拥有的终端会话集合。
  terminalSessions: CommandSessionSummary[];
  // Backend-projected subagent navigation for the selected parent conversation.
  // 当前父会话对应的后端投影子代理导航信息。
  subagentNavigation: WorkbenchSnapshot["subagentNavigation"] | null;
  // Whether the todolist panel is currently expanded.
  // todolist 面板当前是否展开。
  expanded: boolean;
  // Updates the todolist panel expanded state.
  // 更新 todolist 面板展开状态。
  onExpandedChange: (expanded: boolean) => void;
  // Opens or closes the shared Git commit dialog.
  // 打开或关闭共享 Git 提交弹窗。
  onCommitDialogOpenChange: (open: boolean) => void;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Opens one subagent transcript inside the right context panel.
  // 在右侧上下文面板中打开单个子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const hasReviewSummary = Boolean(props.reviewSummary?.branchName);
  const hasTodolist = Boolean(props.todolist?.items.length);
  const terminalSessions = React.useMemo(
    () => props.terminalSessions
      .map((summary) => terminalSessionViewModel(summary, t))
      .filter((session) => session.canStop || session.state === "lost" || session.state === "waiting"),
    [props.terminalSessions, t],
  );
  const terminalSummaryById = React.useMemo(
    () => new Map(props.terminalSessions.map((summary) => [summary.id, summary])),
    [props.terminalSessions],
  );
  const hasTerminalSessions = terminalSessions.length > 0;
  // subagentTasks mirrors the backend unclosed-agent navigation list.
  // subagentTasks 镜像后端未关闭代理导航列表。
  const subagentTasks = React.useMemo(
    () => props.subagentNavigation?.tasks ?? [],
    [props.subagentNavigation?.tasks],
  );
  const hasSubagentTasks = subagentTasks.length > 0;
  // collapsedRowCount controls compact panel height for multi-source summaries.
  // collapsedRowCount 控制多来源摘要下的紧凑面板高度。
  const collapsedRowCount = [
    props.reviewSummary,
    props.todolist,
    hasTerminalSessions,
    hasSubagentTasks,
  ].filter(Boolean).length;
  // renderExpanded follows the requested target state while transitionPhase keeps a short animation marker.
  // renderExpanded 跟随请求的目标状态，transitionPhase 则短暂保留动画标记。
  const [renderExpanded, setRenderExpanded] = React.useState(props.expanded);
  const [transitionPhase, setTransitionPhase] = React.useState<ConversationSessionToolTransitionPhase>(null);
  const previousExpandedRef = React.useRef(props.expanded);
  const transitionTimerRef = React.useRef<number | null>(null);

  // finishTransitionLater clears the morph marker after the current animation duration.
  // finishTransitionLater 在当前动画时长结束后清理形态切换标记。
  const finishTransitionLater = React.useCallback(() => {
    if (transitionTimerRef.current !== null) {
      window.clearTimeout(transitionTimerRef.current);
    }
    transitionTimerRef.current = window.setTimeout(() => {
      transitionTimerRef.current = null;
      setTransitionPhase(null);
    }, CONVERSATION_SESSION_TOOL_TRANSITION_MS);
  }, []);

  React.useEffect(() => {
    if (previousExpandedRef.current === props.expanded) {
      return;
    }
    previousExpandedRef.current = props.expanded;
    setRenderExpanded(props.expanded);
    setTransitionPhase(props.expanded ? "expanding" : "collapsing");
    finishTransitionLater();
  }, [finishTransitionLater, props.expanded]);

  React.useEffect(() => () => {
    if (transitionTimerRef.current !== null) {
      window.clearTimeout(transitionTimerRef.current);
    }
  }, []);

  // requestExpandedChange starts the morph immediately from the click frame.
  // requestExpandedChange 从点击当帧立即启动形态切换。
  const requestExpandedChange = React.useCallback((expanded: boolean) => {
    previousExpandedRef.current = expanded;
    setRenderExpanded(expanded);
    setTransitionPhase(expanded ? "expanding" : "collapsing");
    props.onExpandedChange(expanded);
    finishTransitionLater();
  }, [finishTransitionLater, props]);

  const transitionClassName = transitionPhase
    ? `conversation-session-tool-transitioning is-${transitionPhase}`
    : null;

  if (!hasReviewSummary && !hasTodolist && !hasTerminalSessions && !hasSubagentTasks) {
    return null;
  }

  if (!renderExpanded) {
    return (
      <button
        aria-label={t("sessionTools.expand")}
        className={cn(
          "workspace-tool-panel-menu-collapsed conversation-session-todolist-collapsed conversation-session-tool-collapsed conversation-session-tool-surface",
          collapsedRowCount > 1 && "has-multiple-rows",
          transitionClassName,
        )}
        onClick={() => requestExpandedChange(true)}
        title={t("sessionTools.expand")}
        type="button"
      >
        {props.reviewSummary ? (
          <span className="conversation-session-tool-collapsed-row">
            <FileDiff size={15} />
            <span>{t("sessionTools.changes")}</span>
            {conversationSessionReviewDelta(props.reviewSummary)}
          </span>
        ) : null}
        {props.todolist ? (
          <span className="conversation-session-tool-collapsed-row">
            <ClipboardList size={15} />
            <span>{t("sessionTools.progress")}</span>
            <strong>{props.todolist.completedCount}/{props.todolist.totalCount}</strong>
          </span>
        ) : null}
        {hasTerminalSessions ? (
          <span className="conversation-session-tool-collapsed-row">
            <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(terminalSessions[0].state))} size={15} />
            <span>{t("sessionTools.terminal")}</span>
            <strong>{terminalSessions.length}</strong>
          </span>
        ) : null}
        {hasSubagentTasks ? (
          <span className="conversation-session-tool-collapsed-row">
            <Bot size={15} />
            <span>{t("conversation.subagent.schedule")}</span>
            <strong>{subagentTasks.length}</strong>
          </span>
        ) : null}
        <Maximize2 className="conversation-session-tool-expand-hint" size={12} />
      </button>
    );
  }

  return (
    <aside
      className={cn(
        "workspace-tool-panel-menu conversation-session-todolist-menu conversation-session-tool-surface",
        transitionClassName,
      )}
      aria-label={t("sessionTools.menu")}
    >
      {props.reviewSummary ? (
        <ConversationSessionReviewSection
          commitBusyState={props.commitBusyState}
          commitDialogOpen={props.commitDialogOpen}
          reviewSummary={props.reviewSummary}
          onCollapse={() => requestExpandedChange(false)}
          onCommitDialogOpenChange={props.onCommitDialogOpenChange}
          onOpenReviewChanges={props.onOpenReviewChanges}
        />
      ) : null}
      {hasTerminalSessions ? (
        <ConversationSessionTerminalSection
          showCollapse={!props.reviewSummary}
          summariesById={terminalSummaryById}
          terminalSessions={terminalSessions}
          onCollapse={() => requestExpandedChange(false)}
          onStopCommandSession={props.onStopCommandSession}
        />
      ) : null}
      {props.todolist ? (
        <ConversationSessionProgressSection
          todolist={props.todolist}
          onCollapse={props.reviewSummary || hasTerminalSessions ? undefined : () => requestExpandedChange(false)}
        />
      ) : null}
      {props.subagentNavigation && hasSubagentTasks ? (
        <ConversationSessionSubagentSection
          parentConversationId={props.subagentNavigation.parentConversationId}
          showCollapse={!props.reviewSummary && !hasTerminalSessions && !hasTodolist}
          tasks={subagentTasks}
          onCollapse={() => requestExpandedChange(false)}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
        />
      ) : null}
    </aside>
  );
}

// ConversationSessionReviewSection renders compact Git review information above progress.
// ConversationSessionReviewSection 在进程上方渲染紧凑 Git 审查信息。
function ConversationSessionReviewSection(props: {
  // Whether the shared Git commit dialog is currently open.
  // 共享 Git 提交弹窗当前是否打开。
  commitDialogOpen: boolean;
  // Current shared Git action phase shown by every commit trigger.
  // 每个提交入口展示的当前共享 Git 动作阶段。
  commitBusyState: ConversationSessionCommitBusyState;
  // Review summary rendered by this section.
  // 当前区域渲染的审查摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Opens or closes the shared Git commit dialog.
  // 打开或关闭共享 Git 提交弹窗。
  onCommitDialogOpenChange: (open: boolean) => void;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // hasGitCommitOrPushContent tracks whether the compact Git entry can open a meaningful action.
  // hasGitCommitOrPushContent 判断紧凑 Git 入口是否存在可执行的提交或推送内容。
  const hasGitCommitOrPushContent = props.reviewSummary.stagedChangedFiles > 0
    || props.reviewSummary.unstagedChangedFiles > 0
    || props.reviewSummary.unpushedCommits > 0;
  // commitTriggerDisabled disables the entry while Git is busy or no commit/push action is possible.
  // commitTriggerDisabled 在 Git 忙碌或没有可提交/可推送内容时禁用入口。
  const commitTriggerDisabled = Boolean(props.commitBusyState) || !hasGitCommitOrPushContent;

  return (
    <section className="conversation-session-tool-section conversation-session-review-section">
      <div className="conversation-session-tool-section-title with-action">
        <span>{t("gitCommitDialog.toolsTitle")}</span>
        <button
          aria-label={t("sessionTools.collapse")}
          className="workspace-tool-panel-menu-icon-button"
          onClick={props.onCollapse}
          title={t("sessionTools.collapse")}
          type="button"
        >
          <Minimize2 size={13} />
        </button>
      </div>
      <div className="workspace-tool-panel-menu-list conversation-session-review-list">
        <button className="workspace-tool-panel-menu-row conversation-session-review-row" onClick={props.onOpenReviewChanges} type="button">
          <FileDiff size={15} />
          <span>{t("gitCommitDialog.changes")}</span>
          {conversationSessionReviewDelta(props.reviewSummary)}
        </button>
        {props.reviewSummary.branchName ? (
          <div className="workspace-tool-panel-menu-row conversation-session-review-row">
            <GitBranch size={15} />
            <span>{conversationSessionBranchDisplayName(props.reviewSummary.branchName)}</span>
          </div>
        ) : null}
        <button
          aria-expanded={props.commitDialogOpen}
          className={cn(
            "workspace-tool-panel-menu-row conversation-session-review-row conversation-session-commit-trigger",
            props.commitDialogOpen && "active",
          )}
          disabled={commitTriggerDisabled}
          onClick={() => props.onCommitDialogOpenChange(true)}
          type="button"
        >
          <GitCommitHorizontal size={15} />
          <span>{props.commitBusyState?.label ?? t("gitCommitDialog.open")}</span>
          {props.commitBusyState ? <LoaderCircle className="spin" size={14} /> : null}
        </button>
      </div>
    </section>
  );
}

// ConversationSessionCommitDialog renders the shared Git commit or push dialog.
// ConversationSessionCommitDialog 渲染共享的 Git 提交或推送弹窗。
export function ConversationSessionCommitDialog(props: {
  // Whether the shared Git commit dialog is currently open.
  // 共享 Git 提交弹窗当前是否打开。
  open: boolean;
  // Opens or closes the shared Git commit dialog.
  // 打开或关闭共享 Git 提交弹窗。
  onOpenChange: (open: boolean) => void;
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Current outer Git action phase, if one is already running.
  // 当前外层 Git 动作阶段；存在时表示已有动作正在运行。
  busyState: ConversationSessionCommitBusyState;
  // Review summary used to gate local commit actions.
  // 用于限制本地提交动作的审核摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Updates every shared Git trigger with the active action phase.
  // 使用当前动作阶段更新每个共享 Git 入口。
  onBusyStateChange: (state: ConversationSessionCommitBusyState) => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="conversation-session-commit-dialog">
        <DialogTitle className="sr-only">{t("gitCommitDialog.title")}</DialogTitle>
        <DialogDescription className="sr-only">
          {t("gitCommitDialog.description")}
        </DialogDescription>
        <ConversationSessionCommitPanel
          commitBranchPrefix={props.commitBranchPrefix}
          conversationId={props.conversationId}
          busyState={props.busyState}
          reviewSummary={props.reviewSummary}
          workspacePath={props.workspacePath}
          onClose={() => props.onOpenChange(false)}
          onBusyStateChange={props.onBusyStateChange}
          onRefreshReviewSummary={props.onRefreshReviewSummary}
        />
      </DialogContent>
    </Dialog>
  );
}

// ConversationSessionCommitPanel renders the visual-only commit draft surface.
// ConversationSessionCommitPanel 渲染仅界面状态的提交草稿浮层。
function ConversationSessionCommitPanel(props: {
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Current outer Git action phase, if one is already running.
  // 当前外层 Git 动作阶段；存在时表示已有动作正在运行。
  busyState: ConversationSessionCommitBusyState;
  // Review summary used to gate local commit actions.
  // 用于限制本地提交动作的审核摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Closes the commit dialog after a Git action starts.
  // Git 动作开始后关闭提交弹窗。
  onClose: () => void;
  // Updates the collapsed Git trigger row with the active action phase.
  // 使用当前动作阶段更新折叠 Git 入口行。
  onBusyStateChange: (state: ConversationSessionCommitBusyState) => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
}) {
  const { t } = useI18n();
  // includeUnstaged stores whether unstaged working tree changes are included by default.
  // includeUnstaged 保存默认是否包含工作区未暂存变更。
  const [includeUnstaged, setIncludeUnstaged] = React.useState(props.reviewSummary.unstagedChangedFiles > 0);
  // target stores whether the draft points at the current branch or a new branch.
  // target 保存草稿指向当前分支还是新分支。
  const [target, setTarget] = React.useState<ConversationSessionCommitTarget>("current");
  // branchMenuOpen stores the lightweight branch target menu state.
  // branchMenuOpen 保存轻量分支目标菜单状态。
  const [branchMenuOpen, setBranchMenuOpen] = React.useState(false);
  // branchName stores the editable new branch name shown by the visual draft.
  // branchName 保存界面草稿中展示的可编辑新分支名。
  const [branchName, setBranchName] = React.useState("");
  // branchNameEdited tracks whether the user has overridden the generated branch name.
  // branchNameEdited 标记用户是否已经覆盖自动生成的分支名。
  const [branchNameEdited, setBranchNameEdited] = React.useState(false);
  // commitMessage stores the manually typed commit message draft.
  // commitMessage 保存用户手动输入的提交信息草稿。
  const [commitMessage, setCommitMessage] = React.useState("");
  // lockedAction stores the action confirmed by Ctrl+Enter.
  // lockedAction 保存 Ctrl+Enter 当前确认执行的动作。
  const [lockedAction, setLockedAction] = React.useState<ConversationSessionCommitActionId>("commit");
  // actionButtonRefs store focus targets for keyboard action locking.
  // actionButtonRefs 保存键盘动作锁定使用的焦点目标。
  const actionButtonRefs = React.useRef<Record<ConversationSessionCommitActionId, HTMLButtonElement | null>>({
    commit: null,
    commitPush: null,
    push: null,
  });
  const currentBranchName = props.reviewSummary.branchName
    ? conversationSessionBranchDisplayName(props.reviewSummary.branchName)
    : t("gitCommitDialog.currentBranch");
  const defaultNewBranchName = React.useMemo(
    () => conversationSessionDefaultNewBranchName(currentBranchName, props.reviewSummary.branchNames, props.commitBranchPrefix),
    [currentBranchName, props.commitBranchPrefix, props.reviewSummary.branchNames],
  );
  const hasStagedChanges = props.reviewSummary.stagedChangedFiles > 0;
  const hasUnstagedChanges = props.reviewSummary.unstagedChangedFiles > 0;
  const hasIncludedChanges = hasStagedChanges || (includeUnstaged && hasUnstagedChanges);
  const hasPushOnlyChanges = props.reviewSummary.unpushedCommits > 0;
  const includedAdditions = props.reviewSummary.stagedAdditions
    + (includeUnstaged ? props.reviewSummary.unstagedAdditions : 0);
  const includedDeletions = props.reviewSummary.stagedDeletions
    + (includeUnstaged ? props.reviewSummary.unstagedDeletions : 0);
  const hasValidCommitTarget = target === "current" || branchName.trim().length > 0;
  const includeUnstagedDisabled = !hasUnstagedChanges;
  const effectiveTargetLabel = target === "new" ? (branchName.trim() || t("gitCommitDialog.newBranch")) : currentBranchName;
  const newBranchPlaceholder = `${t("gitCommitDialog.recommendedPrefix")}${defaultNewBranchName}`;
  const backendReady = Boolean(props.conversationId && props.workspacePath);
  const controlsDisabled = props.busyState !== null || !backendReady;
  const commitActionDisabled = controlsDisabled || !hasIncludedChanges || !hasValidCommitTarget;
  const pushOnlyActionDisabled = controlsDisabled || hasIncludedChanges || !hasPushOnlyChanges;

  React.useEffect(() => {
    if (!hasUnstagedChanges) {
      setIncludeUnstaged(false);
    }
  }, [hasUnstagedChanges]);

  React.useEffect(() => {
    if (target === "new" && !branchNameEdited) {
      setBranchName(defaultNewBranchName);
    }
  }, [branchNameEdited, defaultNewBranchName, target]);

  // commitTargetRequest builds the backend target payload from the current branch controls.
  // commitTargetRequest 根据当前分支控件构建后端目标载荷。
  const commitTargetRequest = React.useCallback(() => {
    if (target === "new") {
      return { mode: "new" as const, branchName: branchName.trim() };
    }
    return { mode: "current" as const };
  }, [branchName, target]);

  // runCommitAction executes a commit or commit-and-push when the message draft is ready.
  // runCommitAction 在提交信息草稿就绪后执行提交或提交并推送。
  const runCommitAction = React.useCallback(async (push: boolean) => {
    if (!props.conversationId || !props.workspacePath || props.busyState) {
      return;
    }
    if (!hasIncludedChanges || !hasValidCommitTarget) {
      return;
    }
    props.onClose();
    try {
      let message = commitMessage.trim();
      if (!message) {
        props.onBusyStateChange({ label: t("gitCommitDialog.preparingCommitContent") });
        await Promise.resolve();
        props.onBusyStateChange({ label: t("gitCommitDialog.generatingCommitMessage") });
        const response = await generateGitCommitMessage({
          conversationId: props.conversationId,
          workspacePath: props.workspacePath,
          includeUnstaged,
        });
        message = response?.message?.trim() ?? "";
        if (!message) {
          throw new Error(t("gitCommitDialog.generateEmpty"));
        }
      }
      props.onBusyStateChange({ label: t("gitCommitDialog.committing") });
      const commitResponse = await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        target: commitTargetRequest(),
        includeUnstaged,
        message,
        push: false,
      });
      if (push) {
        props.onBusyStateChange({ label: t("gitCommitDialog.pushingDetailed") });
        const pushResponse = await runGitCommitAction({
          conversationId: props.conversationId,
          workspacePath: props.workspacePath,
          target: { mode: "current" },
          includeUnstaged: false,
          message: "",
          push: true,
        });
        showGitCommitActionToast(pushResponse ?? commitResponse, t("gitCommitDialog.pushDone"));
      } else {
        showGitCommitActionToast(commitResponse, t("gitCommitDialog.actionDone"));
      }
      await props.onRefreshReviewSummary();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      props.onBusyStateChange(null);
    }
  }, [commitMessage, commitTargetRequest, hasIncludedChanges, hasValidCommitTarget, includeUnstaged, props, t]);

  // runPushAction pushes the current branch without creating a commit.
  // runPushAction 推送当前分支且不创建提交。
  const runPushAction = React.useCallback(async () => {
    if (!props.conversationId || !props.workspacePath || props.busyState) {
      return;
    }
    props.onClose();
    props.onBusyStateChange({ label: t("gitCommitDialog.pushingDetailed") });
    try {
      const response = await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        target: { mode: "current" },
        includeUnstaged: false,
        message: "",
        push: true,
      });
      showGitCommitActionToast(response, t("gitCommitDialog.pushDone"));
      await props.onRefreshReviewSummary();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      props.onBusyStateChange(null);
    }
  }, [props, t]);

  const commitActionChoices = React.useMemo(() => ([
    {
      disabled: commitActionDisabled,
      id: "commit" as const,
      run: () => {
        void runCommitAction(false);
      },
    },
    {
      disabled: commitActionDisabled,
      id: "commitPush" as const,
      run: () => {
        void runCommitAction(true);
      },
    },
    {
      disabled: pushOnlyActionDisabled,
      id: "push" as const,
      run: () => {
        void runPushAction();
      },
    },
  ]), [commitActionDisabled, pushOnlyActionDisabled, runCommitAction, runPushAction]);

  React.useEffect(() => {
    const lockedChoice = commitActionChoices.find((choice) => choice.id === lockedAction);
    if (lockedChoice && !lockedChoice.disabled) {
      return;
    }
    const nextChoice = commitActionChoices.find((choice) => !choice.disabled);
    if (nextChoice) {
      setLockedAction(nextChoice.id);
    }
  }, [commitActionChoices, lockedAction]);

  // moveLockedCommitAction rotates the keyboard-confirmed action among enabled choices.
  // moveLockedCommitAction 在可用动作之间轮换键盘确认的锁定项。
  const moveLockedCommitAction = React.useCallback((direction: -1 | 1) => {
    const enabledChoices = commitActionChoices.filter((choice) => !choice.disabled);
    if (!enabledChoices.length) {
      return;
    }
    const currentIndex = enabledChoices.findIndex((choice) => choice.id === lockedAction);
    const safeIndex = currentIndex >= 0 ? currentIndex : 0;
    const nextIndex = (safeIndex + direction + enabledChoices.length) % enabledChoices.length;
    const nextActionId = enabledChoices[nextIndex].id;
    setBranchMenuOpen(false);
    setLockedAction(nextActionId);
    actionButtonRefs.current[nextActionId]?.focus();
  }, [commitActionChoices, lockedAction]);

  // executeLockedCommitAction runs the action currently selected for Ctrl+Enter.
  // executeLockedCommitAction 执行当前为 Ctrl+Enter 锁定的动作。
  const executeLockedCommitAction = React.useCallback(() => {
    const choice = commitActionChoices.find((candidate) => candidate.id === lockedAction);
    if (!choice || choice.disabled) {
      return;
    }
    choice.run();
  }, [commitActionChoices, lockedAction]);

  // handleCommitPanelKeyDown applies dialog-level shortcuts without stealing text editing arrows.
  // handleCommitPanelKeyDown 应用弹窗级快捷键，同时不抢占文本编辑方向键。
  const handleCommitPanelKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      executeLockedCommitAction();
      return;
    }
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") {
      return;
    }
    if (!commitPanelTargetAllowsActionNavigation(event.target)) {
      return;
    }
    event.preventDefault();
    moveLockedCommitAction(event.key === "ArrowUp" ? -1 : 1);
  }, [executeLockedCommitAction, moveLockedCommitAction]);

  return (
    <div className="conversation-session-commit-panel" onKeyDown={handleCommitPanelKeyDown}>
      <div className="conversation-session-commit-header">
        <div className="conversation-session-commit-target">
          <button
            aria-expanded={branchMenuOpen}
            className="conversation-session-commit-target-button"
            onClick={() => setBranchMenuOpen((current) => !current)}
            type="button"
          >
            <GitBranch size={14} />
            <span>{effectiveTargetLabel}</span>
            <ChevronDown size={13} />
          </button>
          {branchMenuOpen ? (
            <div className="conversation-session-commit-branch-menu">
              <span className="conversation-session-commit-branch-menu-title">{t("gitCommitDialog.commitTo")}</span>
              <button
                className="conversation-session-commit-branch-option"
                onClick={() => {
                  setTarget("current");
                  setBranchMenuOpen(false);
                }}
                type="button"
              >
                <GitBranch size={14} />
                <span>{currentBranchName}</span>
                {target === "current" ? <Check size={14} /> : null}
              </button>
              <button
                className="conversation-session-commit-branch-option"
                onClick={() => {
                  setTarget("new");
                  setBranchName(defaultNewBranchName);
                  setBranchNameEdited(false);
                  setBranchMenuOpen(false);
                }}
                type="button"
              >
                <PlusIcon />
                <span>{t("gitCommitDialog.newBranch")}</span>
                {target === "new" ? <Check size={14} /> : null}
              </button>
            </div>
          ) : null}
        </div>
        {hasIncludedChanges ? (
          <strong className="conversation-session-commit-delta">
            <span className="added">+{includedAdditions}</span>
            <span className="removed">-{includedDeletions}</span>
          </strong>
        ) : (
          <strong className="conversation-session-commit-empty">{t("gitCommitDialog.noChanges")}</strong>
        )}
      </div>
      {target === "new" ? (
        <input
          aria-label={t("gitCommitDialog.branchName")}
          className="conversation-session-commit-branch-input"
          onChange={(event) => {
            setBranchNameEdited(true);
            setBranchName(event.currentTarget.value);
          }}
          placeholder={newBranchPlaceholder}
          value={branchName}
        />
      ) : null}
      <textarea
        aria-label={t("gitCommitDialog.message")}
        className="conversation-session-commit-message"
        disabled={controlsDisabled || !hasIncludedChanges}
        onChange={(event) => {
          setCommitMessage(event.currentTarget.value);
        }}
        placeholder={t("gitCommitDialog.messagePlaceholder")}
        rows={2}
        value={commitMessage}
      />
      <label className={cn("conversation-session-commit-checkbox", includeUnstagedDisabled && "disabled")}>
        <input
          checked={includeUnstaged}
          disabled={includeUnstagedDisabled}
          onChange={(event) => setIncludeUnstaged(event.currentTarget.checked)}
          type="checkbox"
        />
        <span>{t("gitCommitDialog.includeUnstaged")}</span>
      </label>
      <div className="conversation-session-commit-actions">
        <button
          aria-pressed={lockedAction === "commit"}
          className={cn("conversation-session-commit-action", lockedAction === "commit" && "locked")}
          disabled={commitActionDisabled}
          onClick={() => void runCommitAction(false)}
          onFocus={() => setLockedAction("commit")}
          ref={(element) => {
            actionButtonRefs.current.commit = element;
          }}
          type="button"
        >
          <GitCommitHorizontal size={15} />
          <span>{t("gitCommitDialog.commit")}</span>
          {lockedAction === "commit" ? <kbd>Ctrl+↵</kbd> : null}
        </button>
        <button
          aria-pressed={lockedAction === "commitPush"}
          className={cn("conversation-session-commit-action", lockedAction === "commitPush" && "locked")}
          disabled={commitActionDisabled}
          onClick={() => void runCommitAction(true)}
          onFocus={() => setLockedAction("commitPush")}
          ref={(element) => {
            actionButtonRefs.current.commitPush = element;
          }}
          type="button"
        >
          <CloudUpload size={15} />
          <span>{t("gitCommitDialog.commitAndPush")}</span>
          {lockedAction === "commitPush" ? <kbd>Ctrl+↵</kbd> : null}
        </button>
        <button
          aria-pressed={lockedAction === "push"}
          className={cn("conversation-session-commit-action", lockedAction === "push" && "locked")}
          disabled={pushOnlyActionDisabled}
          onClick={() => void runPushAction()}
          onFocus={() => setLockedAction("push")}
          ref={(element) => {
            actionButtonRefs.current.push = element;
          }}
          type="button"
        >
          <CloudUpload size={15} />
          <span>{t("gitCommitDialog.push")}</span>
          {lockedAction === "push" ? <kbd>Ctrl+↵</kbd> : null}
        </button>
      </div>
    </div>
  );
}

// showGitCommitActionToast reports a completed Git action without forcing the dialog to stay open.
// showGitCommitActionToast 在不保持弹窗打开的情况下报告完成的 Git 操作。
function showGitCommitActionToast(response: GitCommitActionResponse | null | undefined, title: string): void {
  const details = [
    response?.commitHash ? `# ${response.commitHash}` : null,
    response?.commitMessage ? response.commitMessage : null,
  ].filter((item): item is string => Boolean(item));
  toast.success(title, {
    description: details.length > 0 ? details.join("\n") : response?.summary,
  });
}

// commitPanelTargetAllowsActionNavigation returns whether arrow keys should switch locked actions.
// commitPanelTargetAllowsActionNavigation 返回方向键是否应切换锁定动作。
function commitPanelTargetAllowsActionNavigation(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return true;
  }
  const tagName = target.tagName.toLowerCase();
  return tagName !== "select" && !target.isContentEditable;
}

// ConversationSessionSubagentSection renders subagent dispatch rows in the session tool panel.
// ConversationSessionSubagentSection 在会话工具面板中渲染子代理调度行。
function ConversationSessionSubagentSection(props: {
  // Parent conversation that owns these subagent tasks.
  // 当前子代理任务所属的父会话。
  parentConversationId: string;
  // Visible active task rows.
  // 可见的活跃任务行。
  tasks: ConversationSessionSubagentTask[];
  // Whether this section owns the panel collapse action.
  // 当前区域是否承载面板折叠动作。
  showCollapse: boolean;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
  // Opens one subagent transcript inside the right context panel.
  // 在右侧上下文面板中打开单个子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <section className="conversation-session-tool-section conversation-session-subagent-section">
      <div className={cn("conversation-session-tool-section-title", props.showCollapse && "with-action")}>
        <span className="conversation-session-tool-section-title-main">
          <Bot size={13} />
          <span>{t("conversation.subagent.schedule")}</span>
          <strong>{props.tasks.length}</strong>
        </span>
        {props.showCollapse ? (
          <button
            aria-label={t("sessionTools.collapse")}
            className="workspace-tool-panel-menu-icon-button"
            onClick={props.onCollapse}
            title={t("sessionTools.collapse")}
            type="button"
          >
            <Minimize2 size={13} />
          </button>
        ) : null}
      </div>
      <div className="workspace-tool-panel-menu-list conversation-session-subagent-list">
        {props.tasks.map((task) => (
          <Tooltip key={task.taskId}>
            <TooltipTrigger asChild>
              <button
                className={cn("workspace-tool-panel-menu-row conversation-session-subagent-row", task.isRunning && "running")}
                onClick={() => void props.onOpenSubagentPanel(props.parentConversationId, task.taskId)}
                style={conversationSessionSubagentAccentStyle(task)}
                type="button"
              >
                <Boxes size={15} />
                <span className="conversation-session-subagent-label">{task.label}</span>
                <span className={cn("conversation-session-subagent-state", task.runState)}>
                  {conversationSessionSubagentRunStateLabel(task.runState, t)}
                </span>
              </button>
            </TooltipTrigger>
            <TooltipContent className="conversation-session-subagent-tooltip" side="left" sideOffset={8}>
              <pre>{conversationSessionSubagentTooltip(task, t)}</pre>
            </TooltipContent>
          </Tooltip>
        ))}
      </div>
    </section>
  );
}

// conversationSessionSubagentTooltip builds a detailed hover summary for one unclosed agent row.
// conversationSessionSubagentTooltip 为单条未关闭智能体行构建详细悬浮摘要。
function conversationSessionSubagentTooltip(
  // Task row from the backend navigation snapshot.
  // 后端导航快照中的任务行。
  task: ConversationSessionSubagentTask,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  return [
    task.label,
    `${t("conversation.subagent.detail.status")}: ${conversationSessionSubagentRunStateLabel(task.runState, t)}`,
    `${t("conversation.subagent.detail.name")}: ${task.publicTaskId}`,
    task.description ? `${t("conversation.subagent.detail.description")}: ${task.description}` : null,
  ].filter((line): line is string => Boolean(line)).join("\n");
}

// conversationSessionSubagentAccentStyle returns a stable colorful accent for active subagent rows.
// conversationSessionSubagentAccentStyle 为活跃子代理行返回稳定的彩色强调样式。
function conversationSessionSubagentAccentStyle(
  // Task row used to derive active-state color.
  // 用于推导活跃状态颜色的任务行。
  task: ConversationSessionSubagentTask,
): React.CSSProperties | undefined {
  if (!task.isRunning) {
    return undefined;
  }
  return {
    "--conversation-session-subagent-accent": conversationSessionSubagentAccentColor(task.taskId),
  } as React.CSSProperties;
}

// conversationSessionSubagentAccentColor maps a task id to one stable lively color.
// conversationSessionSubagentAccentColor 将任务标识映射为一个稳定的活跃颜色。
function conversationSessionSubagentAccentColor(
  // Stable task id from the backend projection.
  // 后端投影中的稳定任务标识。
  taskId: string,
): string {
  const palette = [
    "hsl(199 89% 48%)",
    "hsl(262 83% 58%)",
    "hsl(330 81% 55%)",
    "hsl(18 88% 54%)",
    "hsl(142 66% 39%)",
    "hsl(45 93% 47%)",
  ];
  let hash = 0;
  for (const char of taskId) {
    hash = ((hash * 31) + char.charCodeAt(0)) >>> 0;
  }
  return palette[hash % palette.length] ?? palette[0];
}

// conversationSessionSubagentRunStateLabel maps backend run-state keys onto compact localized labels.
// conversationSessionSubagentRunStateLabel 将后端运行状态键映射为紧凑本地化标签。
function conversationSessionSubagentRunStateLabel(
  // Backend run-state key.
  // 后端运行状态键。
  runState: ConversationSessionSubagentTask["runState"],
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string {
  switch (runState) {
    case "running":
      return t("conversation.subagent.state.running");
    case "stopped":
      return t("conversation.subagent.state.stopped");
    case "failed":
      return t("conversation.subagent.state.failed");
    case "completed":
    default:
      return t("conversation.subagent.state.completed");
  }
}

// PlusIcon renders the compact plus mark used by the branch target menu.
// PlusIcon 渲染分支目标菜单中使用的紧凑加号。
function PlusIcon() {
  return (
    <svg aria-hidden="true" className="conversation-session-commit-plus-icon" viewBox="0 0 16 16">
      <path d="M8 3.5v9M3.5 8h9" fill="none" stroke="currentColor" strokeLinecap="round" strokeWidth="1.5" />
    </svg>
  );
}

// ConversationSessionProgressSection renders the model-authored session progress plan.
// ConversationSessionProgressSection 渲染模型写入的会话进程计划。
function ConversationSessionProgressSection(props: {
  // todolist rendered as session progress.
  // 作为会话进程渲染的 todolist。
  todolist: SessionTodolistSummary;
  // Optional collapse action used when progress is the first section.
  // 当进程为第一个区域时使用的可选折叠动作。
  onCollapse?: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <section className="conversation-session-tool-section conversation-session-progress-section">
      <div className={cn("conversation-session-tool-section-title", props.onCollapse && "with-action")}>
        <span className="conversation-session-tool-section-title-main">
          <span>{t("sessionTools.progress")}</span>
          <strong>{props.todolist.completedCount}/{props.todolist.totalCount}</strong>
        </span>
        {props.onCollapse ? (
          <button
            aria-label={t("sessionTools.collapse")}
            className="workspace-tool-panel-menu-icon-button"
            onClick={props.onCollapse}
            title={t("sessionTools.collapse")}
            type="button"
          >
            <Minimize2 size={13} />
          </button>
        ) : null}
      </div>
      <div className="workspace-tool-panel-menu-list conversation-session-todolist-list">
        {props.todolist.items.map((item, index) => (
          <ConversationSessionTodolistRow item={item} key={`${index}-${item.status}-${item.step}`} />
        ))}
      </div>
    </section>
  );
}

// ConversationSessionTodolistRow renders one todolist item with its current model-authored status.
// ConversationSessionTodolistRow 使用模型写入的当前状态渲染单条 todolist。
function ConversationSessionTodolistRow(props: {
  // todolist item rendered by this row.
  // 当前行渲染的 todolist 条目。
  item: SessionTodolistItemSummary;
}) {
  return (
    <div className={cn("conversation-session-todolist-row", `status-${props.item.status}`)}>
      {conversationSessionTodolistStatusIcon(props.item.status)}
      <span>{props.item.step}</span>
    </div>
  );
}

// conversationSessionTodolistStatusIcon returns the compact icon for one todolist status.
// conversationSessionTodolistStatusIcon 返回单个 todolist 状态对应的紧凑图标。
function conversationSessionTodolistStatusIcon(status: SessionTodolistItemSummary["status"]): React.ReactNode {
  if (status === "completed") {
    return <CheckCircle2 size={15} />;
  }
  if (status === "in_progress") {
    return <LoaderCircle className="spin" size={15} />;
  }
  return <Circle size={15} />;
}

// conversationSessionReviewDelta renders colored added and deleted line counters.
// conversationSessionReviewDelta 渲染带颜色的新增与删除行数。
function conversationSessionReviewDelta(reviewSummary: ConversationSessionReviewSummary): React.ReactNode {
  return (
    <strong className="conversation-session-review-delta">
      <span className="added">+{reviewSummary.additions}</span>
      <span className="removed">-{reviewSummary.deletions}</span>
    </strong>
  );
}

// conversationSessionBranchDisplayName removes noisy Git ref and origin prefixes for compact review display.
// conversationSessionBranchDisplayName 移除噪声 Git ref 与 origin 前缀以紧凑展示审查分支。
function conversationSessionBranchDisplayName(name: string): string {
  const shortName = name
    .replace(/^refs\/heads\//, "")
    .replace(/^refs\/remotes\//, "");
  return shortName.startsWith("origin/") ? shortName.slice("origin/".length) : shortName;
}

// conversationSessionDefaultNewBranchName builds the first unused recommended commit branch name.
// conversationSessionDefaultNewBranchName 生成第一个未占用的推荐提交分支名。
function conversationSessionDefaultNewBranchName(currentBranchName: string, branchNames: string[], configuredPrefix: string): string {
  const normalizedCurrentBranch = conversationSessionNormalizeBranchName(currentBranchName);
  const existingBranches = new Set(branchNames.map(conversationSessionNormalizeBranchName));
  existingBranches.add(normalizedCurrentBranch);
  const slashIndex = normalizedCurrentBranch.lastIndexOf("/");
  const branchNamespace = slashIndex >= 0 ? normalizedCurrentBranch.slice(0, slashIndex + 1) : configuredPrefix;
  const branchLeafName = slashIndex >= 0 ? normalizedCurrentBranch.slice(slashIndex + 1) : normalizedCurrentBranch;
  const baseBranchName = `${branchNamespace}${branchLeafName}`;
  if (!existingBranches.has(baseBranchName)) {
    return baseBranchName;
  }
  for (let suffix = 2; ; suffix += 1) {
    const candidateBranchName = `${baseBranchName}${suffix}`;
    if (!existingBranches.has(candidateBranchName)) {
      return candidateBranchName;
    }
  }
}

// conversationSessionNormalizeBranchName normalizes Git ref labels before branch-name comparison.
// conversationSessionNormalizeBranchName 在分支名比较前归一化 Git 引用标签。
function conversationSessionNormalizeBranchName(name: string): string {
  return conversationSessionBranchDisplayName(name).trim();
}
