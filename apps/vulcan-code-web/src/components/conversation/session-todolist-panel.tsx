import React from "react";
import {
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
  Square,
  TerminalSquare,
} from "lucide-react";
import {
  terminalSessionViewModel,
  terminalStateClass,
  type TerminalSessionViewModel,
} from "@/components/conversation/conversation-terminal-model";
import { TerminalOutputView } from "@/components/conversation/conversation-terminal-output";
import { TerminalStopConfirmDialog } from "@/components/conversation/terminal-stop-confirm-dialog";
import {
  generateGitCommitMessage,
  getCommandSessionSnapshot,
  runGitCommitAction,
  sendCommandSessionInput,
  type TerminalInputEvent,
} from "@/service/conversation-actions-service";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { CommandSessionSummary, SessionTodolistItemSummary, SessionTodolistSummary } from "@/types";

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

// ConversationSessionTodolistPanel renders the session todolist inside the main conversation surface.
// ConversationSessionTodolistPanel 在主会话界面内渲染当前会话的 todolist。
export function ConversationSessionTodolistPanel(props: {
  // Current todolist summary projected from the selected session runtime state.
  // 从当前选中会话运行时状态投影出的 todolist摘要。
  todolist: SessionTodolistSummary | null;
  // Current Git review summary projected from the selected workspace.
  // 从当前选中工作区投影出的 Git 审查摘要。
  reviewSummary: ConversationSessionReviewSummary | null;
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Current terminal sessions owned by the selected conversation.
  // 当前选中会话拥有的终端会话集合。
  terminalSessions: CommandSessionSummary[];
  // Whether the todolist panel is currently expanded.
  // todolist 面板当前是否展开。
  expanded: boolean;
  // Updates the todolist panel expanded state.
  // 更新 todolist 面板展开状态。
  onExpandedChange: (expanded: boolean) => void;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  const hasReviewSummary = Boolean(props.reviewSummary?.branchName);
  const hasTodolist = Boolean(props.todolist?.items.length);
  const terminalSessions = React.useMemo(
    () => props.terminalSessions
      .map(terminalSessionViewModel)
      .filter((session) => session.canStop || session.state === "lost" || session.state === "waiting"),
    [props.terminalSessions],
  );
  const terminalSummaryById = React.useMemo(
    () => new Map(props.terminalSessions.map((summary) => [summary.id, summary])),
    [props.terminalSessions],
  );
  const hasTerminalSessions = terminalSessions.length > 0;
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

  if (!hasReviewSummary && !hasTodolist && !hasTerminalSessions) {
    return null;
  }

  if (!renderExpanded) {
    return (
      <button
        aria-label="展开会话工具"
        className={cn(
          "workspace-tool-panel-menu-collapsed conversation-session-todolist-collapsed conversation-session-tool-collapsed conversation-session-tool-surface",
          (hasReviewSummary || hasTerminalSessions) && hasTodolist && "has-progress",
          transitionClassName,
        )}
        onClick={() => requestExpandedChange(true)}
        title="展开会话工具"
        type="button"
      >
        {props.reviewSummary ? (
          <span className="conversation-session-tool-collapsed-row">
            <FileDiff size={15} />
            <span>更改</span>
            {conversationSessionReviewDelta(props.reviewSummary)}
          </span>
        ) : null}
        {props.todolist ? (
          <span className="conversation-session-tool-collapsed-row">
            <ClipboardList size={15} />
            <span>进程</span>
            <strong>{props.todolist.completedCount}/{props.todolist.totalCount}</strong>
          </span>
        ) : null}
        {hasTerminalSessions ? (
          <span className="conversation-session-tool-collapsed-row">
            <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(terminalSessions[0].state))} size={15} />
            <span>终端</span>
            <strong>{terminalSessions.length}</strong>
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
      aria-label="会话工具菜单"
    >
      {props.reviewSummary ? (
        <ConversationSessionReviewSection
          commitBranchPrefix={props.commitBranchPrefix}
          conversationId={props.conversationId}
          reviewSummary={props.reviewSummary}
          workspacePath={props.workspacePath}
          onCollapse={() => requestExpandedChange(false)}
          onOpenReviewChanges={props.onOpenReviewChanges}
          onRefreshReviewSummary={props.onRefreshReviewSummary}
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
    </aside>
  );
}

// ConversationSessionTerminalSection renders active terminal sessions in the shared floating session tool panel.
// ConversationSessionTerminalSection 在共享会话浮动工具面板中渲染活跃终端会话。
function ConversationSessionTerminalSection(props: {
  // Normalized terminal session rows.
  // 归一化后的终端会话行。
  terminalSessions: TerminalSessionViewModel[];
  // Original summaries keyed by session id for owner metadata.
  // 按会话标识索引的原始摘要，用于归属元数据。
  summariesById: Map<string, CommandSessionSummary>;
  // Whether this section owns the panel collapse action.
  // 当前区域是否拥有面板折叠动作。
  showCollapse: boolean;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  return (
    <section className="conversation-session-tool-section conversation-session-terminal-section">
      <div className={cn("conversation-session-tool-section-title", props.showCollapse && "with-action")}>
        <span className="conversation-session-tool-section-title-main">
          <span>终端</span>
          <strong>{props.terminalSessions.length}</strong>
        </span>
        {props.showCollapse ? (
          <button
            aria-label="折叠会话工具"
            className="workspace-tool-panel-menu-icon-button"
            onClick={props.onCollapse}
            title="折叠会话工具"
            type="button"
          >
            <Minimize2 size={13} />
          </button>
        ) : null}
      </div>
      <div className="workspace-tool-panel-menu-list conversation-session-terminal-list">
        {props.terminalSessions.map((session) => (
          <ConversationSessionTerminalRow
            key={session.id}
            session={session}
            summary={props.summariesById.get(session.id) ?? null}
            onStopCommandSession={props.onStopCommandSession}
          />
        ))}
      </div>
    </section>
  );
}

// ConversationSessionTerminalRow renders one terminal session row with status and force-stop action.
// ConversationSessionTerminalRow 渲染单条带状态与强制停止动作的终端会话。
function ConversationSessionTerminalRow(props: {
  // Normalized terminal session row.
  // 归一化后的终端会话行。
  session: TerminalSessionViewModel;
  // Original summary used for owner metadata.
  // 用于归属元数据的原始摘要。
  summary: CommandSessionSummary | null;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
}) {
  const [stopping, setStopping] = React.useState(false);
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  const [screenOpen, setScreenOpen] = React.useState(false);
  const handleStopRequest = React.useCallback(() => {
    if (!props.session.canStop || stopping) {
      return;
    }
    setConfirmOpen(true);
  }, [props.session.canStop, stopping]);
  const handleConfirmStop = React.useCallback(async () => {
    if (!props.session.canStop || stopping) {
      return;
    }
    setStopping(true);
    try {
      await props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null);
      setConfirmOpen(false);
    } finally {
      setStopping(false);
    }
  }, [props, stopping]);
  const handleConfirmOpenChange = React.useCallback((open: boolean) => {
    if (stopping) {
      return;
    }
    setConfirmOpen(open);
  }, [stopping]);
  const tooltipText = terminalSessionTooltipText(props.session, props.summary);
  return (
    <div className="workspace-tool-panel-menu-row conversation-session-terminal-row">
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="conversation-session-terminal-summary">
            <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(props.session.state))} size={15} />
            <span className="conversation-session-terminal-name">{props.session.name}</span>
          </div>
        </TooltipTrigger>
        <TooltipContent className="conversation-session-terminal-tooltip" side="left" sideOffset={8}>
          <pre>{tooltipText}</pre>
        </TooltipContent>
      </Tooltip>
      <button
        aria-label="打开终端屏幕"
        className="conversation-session-terminal-action conversation-session-terminal-screen-button"
        disabled={!props.summary}
        onClick={() => setScreenOpen(true)}
        title={props.summary ? "打开终端屏幕" : "暂无终端屏幕"}
        type="button"
      >
        <Maximize2 size={12} />
      </button>
      <button
        aria-label="强制终止终端会话"
        className="conversation-session-terminal-action conversation-session-terminal-stop-button"
        disabled={!props.session.canStop || stopping}
        onClick={handleStopRequest}
        title={props.session.canStop ? "强制终止" : props.session.label}
        type="button"
      >
        <Square fill="currentColor" size={11} strokeWidth={0} />
      </button>
      <TerminalStopConfirmDialog
        command={props.session.command}
        open={confirmOpen}
        sessionName={props.session.name}
        stopping={stopping}
        onConfirm={handleConfirmStop}
        onOpenChange={handleConfirmOpenChange}
      />
      <TerminalScreenDialog
        open={screenOpen}
        session={props.session}
        summary={props.summary}
        onOpenChange={setScreenOpen}
      />
    </div>
  );
}

// TerminalScreenDialog renders the optional floating terminal screen view.
// TerminalScreenDialog 渲染可选的浮动终端屏幕视图。
function TerminalScreenDialog(props: {
  // Whether the floating terminal screen is open.
  // 浮动终端屏幕是否打开。
  open: boolean;
  // Normalized terminal session row.
  // 归一化后的终端会话行。
  session: TerminalSessionViewModel;
  // Original backend summary with raw screen fields.
  // 带原始屏幕字段的后端摘要。
  summary: CommandSessionSummary | null;
  // Updates the floating terminal screen open state.
  // 更新浮动终端屏幕打开状态。
  onOpenChange: (open: boolean) => void;
}) {
  const [sending, setSending] = React.useState(false);
  const [liveSummary, setLiveSummary] = React.useState<CommandSessionSummary | null>(props.summary);
  React.useEffect(() => {
    setLiveSummary(props.summary);
  }, [props.summary]);
  const refreshLiveSummary = React.useCallback(async () => {
    const nextSummary = await getCommandSessionSnapshot(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null);
    if (nextSummary) {
      setLiveSummary(nextSummary);
    }
  }, [liveSummary?.ownerSessionId, props.session.id, props.summary?.ownerSessionId]);
  React.useEffect(() => {
    if (!props.open) {
      return;
    }
    let cancelled = false;
    const poll = async () => {
      try {
        const nextSummary = await getCommandSessionSnapshot(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null);
        if (!cancelled && nextSummary) {
          setLiveSummary(nextSummary);
        }
      } catch (error) {
        console.warn("Failed to refresh command session screen snapshot.", error);
      }
    };
    void poll();
    const intervalId = window.setInterval(() => {
      void poll();
    }, 250);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [liveSummary?.ownerSessionId, props.open, props.session.id, props.summary?.ownerSessionId]);
  const screenText = liveSummary?.screenText ?? "";
  const backend = liveSummary?.backend ?? props.summary?.backend ?? "unknown";
  const sizeLabel = liveSummary?.screenRows && liveSummary?.screenCols
    ? `${liveSummary.screenRows}x${liveSummary.screenCols}`
    : "unknown";
  const handleKeyDown = React.useCallback(async (event: React.KeyboardEvent<HTMLPreElement>) => {
    const input = terminalInputEventFromKeyboardEvent(event);
    if (!input || sending) {
      return;
    }
    event.preventDefault();
    setSending(true);
    try {
      await sendCommandSessionInput(props.session.id, liveSummary?.ownerSessionId ?? props.summary?.ownerSessionId ?? null, [input]);
      await refreshLiveSummary();
    } finally {
      setSending(false);
    }
  }, [liveSummary?.ownerSessionId, props.session.id, props.summary?.ownerSessionId, refreshLiveSummary, sending]);
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="conversation-session-terminal-screen-dialog">
        <DialogTitle>{props.session.name}</DialogTitle>
        <DialogDescription>
          {backend} · {sizeLabel} · frame {liveSummary?.screenFrameSeq ?? props.summary?.screenFrameSeq ?? 0}
        </DialogDescription>
        <TerminalOutputView
          className="conversation-session-terminal-screen-frame"
          emptyText="暂无终端屏幕内容"
          onKeyDown={handleKeyDown}
          suppressBlackBackground
          tabIndex={0}
          text={screenText}
        />
      </DialogContent>
    </Dialog>
  );
}

// terminalInputEventFromKeyboardEvent converts browser keyboard events into terminal input events.
// terminalInputEventFromKeyboardEvent 将浏览器键盘事件转换为终端输入事件。
function terminalInputEventFromKeyboardEvent(event: React.KeyboardEvent): TerminalInputEvent | null {
  const keyName = terminalKeyNameFromBrowserKey(event.key);
  if (event.ctrlKey || event.altKey || event.metaKey || (event.shiftKey && keyName && event.key !== "Tab")) {
    if (!keyName && event.key.length !== 1) {
      return null;
    }
    return {
      kind: "chord",
      key: keyName ?? event.key,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      meta: event.metaKey,
    };
  }
  if (keyName) {
    return { kind: "key", key: keyName };
  }
  if (event.key.length === 1) {
    return { kind: "text", text: event.key };
  }
  return null;
}

// terminalKeyNameFromBrowserKey maps browser key names to backend key names.
// terminalKeyNameFromBrowserKey 将浏览器按键名映射为后端按键名。
function terminalKeyNameFromBrowserKey(key: string): string | null {
  if (/^F([1-9]|1[0-2])$/.test(key)) {
    return key;
  }
  switch (key) {
    case "Enter":
    case "Tab":
    case "Backspace":
    case "Delete":
    case "Home":
    case "End":
      return key;
    case "Escape":
      return "Esc";
    case "ArrowUp":
    case "ArrowDown":
    case "ArrowLeft":
    case "ArrowRight":
      return key;
    case "PageUp":
    case "PageDown":
    case "Insert":
      return key;
    default:
      return null;
  }
}

// terminalSessionTooltipText builds a complete hover description for one compact terminal row.
// terminalSessionTooltipText 为单条紧凑终端行构建完整悬浮说明。
function terminalSessionTooltipText(
  // Normalized terminal row shown in the panel.
  // 面板中展示的归一化终端行。
  session: TerminalSessionViewModel,
  // Original backend summary with raw runtime fields.
  // 带原始运行时字段的后端摘要。
  summary: CommandSessionSummary | null,
): string {
  const lines = [
    `状态：${session.label}`,
    `命令：${session.command}`,
  ];
  if (summary?.cwd) {
    lines.push(`目录：${summary.cwd}`);
  }
  if (summary?.shellLabel) {
    lines.push(`Shell：${summary.shellLabel}`);
  }
  if (summary?.backend) {
    lines.push(`后端：${summary.backend}`);
  }
  if (summary?.screenFrameSeq) {
    lines.push(`屏幕帧：${summary.screenFrameSeq}`);
  }
  if (typeof session.exitCode === "number") {
    lines.push(`退出码：${session.exitCode}`);
  }
  if (summary?.failureReason) {
    lines.push(`错误：${summary.failureReason}`);
  }
  if (session.preview) {
    lines.push(`输出：${session.preview}`);
  }
  lines.push(`会话：${session.id}`);
  return lines.join("\n");
}

// ConversationSessionReviewSection renders compact Git review information above progress.
// ConversationSessionReviewSection 在进程上方渲染紧凑 Git 审查信息。
function ConversationSessionReviewSection(props: {
  // Shared Git commit new-branch prefix projected from service configuration.
  // 从服务端配置投影出的共享 Git 提交新分支前缀。
  commitBranchPrefix: string;
  // Selected conversation id that owns Git commit generation.
  // 拥有 Git 提交生成动作的当前会话标识。
  conversationId: string | null;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
  // Review summary rendered by this section.
  // 当前区域渲染的审查摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Refreshes the compact Git review summary after backend Git actions.
  // 后端 Git 动作完成后刷新紧凑 Git 审查摘要。
  onRefreshReviewSummary: () => Promise<void>;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
}) {
  const { t } = useI18n();
  // commitPanelOpen stores whether the local commit draft surface is visible.
  // commitPanelOpen 保存本地提交草稿界面是否可见。
  const [commitPanelOpen, setCommitPanelOpen] = React.useState(false);

  return (
    <section className="conversation-session-tool-section conversation-session-review-section">
      <div className="conversation-session-tool-section-title with-action">
        <span>{t("gitCommitDialog.toolsTitle")}</span>
        <button
          aria-label="折叠会话工具"
          className="workspace-tool-panel-menu-icon-button"
          onClick={props.onCollapse}
          title="折叠会话工具"
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
        <Dialog open={commitPanelOpen} onOpenChange={setCommitPanelOpen}>
          <DialogTrigger asChild>
            <button
              aria-expanded={commitPanelOpen}
              className={cn(
                "workspace-tool-panel-menu-row conversation-session-review-row conversation-session-commit-trigger",
                commitPanelOpen && "active",
              )}
              type="button"
            >
              <GitCommitHorizontal size={15} />
              <span>{t("gitCommitDialog.open")}</span>
            </button>
          </DialogTrigger>
          <DialogContent className="conversation-session-commit-dialog">
            <DialogTitle className="sr-only">{t("gitCommitDialog.title")}</DialogTitle>
            <DialogDescription className="sr-only">
              {t("gitCommitDialog.description")}
            </DialogDescription>
            <ConversationSessionCommitPanel
              commitBranchPrefix={props.commitBranchPrefix}
              conversationId={props.conversationId}
              reviewSummary={props.reviewSummary}
              workspacePath={props.workspacePath}
              onRefreshReviewSummary={props.onRefreshReviewSummary}
            />
          </DialogContent>
        </Dialog>
      </div>
    </section>
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
  // Review summary used to gate local commit actions.
  // 用于限制本地提交动作的审核摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Workspace path used to resolve the Git repository on the backend.
  // 后端用于解析 Git 仓库的工作区路径。
  workspacePath: string | null;
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
  // busyAction stores the backend Git action currently in progress.
  // busyAction 保存当前正在执行的后端 Git 动作。
  const [busyAction, setBusyAction] = React.useState<"generate" | "commit" | "commitPush" | "push" | null>(null);
  // statusMessage stores the latest successful backend action summary.
  // statusMessage 保存最近一次成功后端动作摘要。
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null);
  // errorMessage stores the latest failed backend action summary.
  // errorMessage 保存最近一次失败后端动作摘要。
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);
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
  const includedAdditions = props.reviewSummary.stagedAdditions
    + (includeUnstaged ? props.reviewSummary.unstagedAdditions : 0);
  const includedDeletions = props.reviewSummary.stagedDeletions
    + (includeUnstaged ? props.reviewSummary.unstagedDeletions : 0);
  const hasValidCommitTarget = target === "current" || branchName.trim().length > 0;
  const includeUnstagedDisabled = !hasUnstagedChanges;
  const effectiveTargetLabel = target === "new" ? (branchName.trim() || t("gitCommitDialog.newBranch")) : currentBranchName;
  const newBranchPlaceholder = `${t("gitCommitDialog.recommendedPrefix")}${defaultNewBranchName}`;
  const backendReady = Boolean(props.conversationId && props.workspacePath);
  const controlsDisabled = busyAction !== null || !backendReady;

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

  // generateCommitMessage fills the draft message without mutating the repository.
  // generateCommitMessage 在不修改仓库的情况下填充提交信息草稿。
  const generateCommitMessage = React.useCallback(async () => {
    if (!props.conversationId || !props.workspacePath || busyAction) {
      return false;
    }
    setBusyAction("generate");
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const response = await generateGitCommitMessage({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        includeUnstaged,
      });
      if (!response?.message) {
        throw new Error(t("gitCommitDialog.generateEmpty"));
      }
      setCommitMessage(response.message);
      setStatusMessage(response.inputWasTruncated
        ? `${t("gitCommitDialog.generatedTruncated")} ${response.estimatedInputTokens} tokens.`
        : `${t("gitCommitDialog.generated")} ${response.estimatedInputTokens} tokens.`);
      return true;
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
      return false;
    } finally {
      setBusyAction(null);
    }
  }, [busyAction, includeUnstaged, props.conversationId, props.workspacePath, t]);

  // runCommitAction executes a commit or commit-and-push when the message draft is ready.
  // runCommitAction 在提交信息草稿就绪后执行提交或提交并推送。
  const runCommitAction = React.useCallback(async (push: boolean) => {
    if (!props.conversationId || !props.workspacePath || busyAction) {
      return;
    }
    if (!commitMessage.trim()) {
      await generateCommitMessage();
      return;
    }
    setBusyAction(push ? "commitPush" : "commit");
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        target: commitTargetRequest(),
        includeUnstaged,
        message: commitMessage,
        push,
      });
      setStatusMessage(push ? t("gitCommitDialog.pushDone") : t("gitCommitDialog.actionDone"));
      await props.onRefreshReviewSummary();
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyAction(null);
    }
  }, [busyAction, commitMessage, commitTargetRequest, generateCommitMessage, includeUnstaged, props, t]);

  // runPushAction pushes the current branch without creating a commit.
  // runPushAction 推送当前分支且不创建提交。
  const runPushAction = React.useCallback(async () => {
    if (!props.conversationId || !props.workspacePath || busyAction) {
      return;
    }
    setBusyAction("push");
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await runGitCommitAction({
        conversationId: props.conversationId,
        workspacePath: props.workspacePath,
        target: { mode: "current" },
        includeUnstaged: false,
        message: "",
        push: true,
      });
      setStatusMessage(t("gitCommitDialog.pushDone"));
      await props.onRefreshReviewSummary();
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusyAction(null);
    }
  }, [busyAction, props, t]);

  return (
    <div className="conversation-session-commit-panel">
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
        onChange={(event) => {
          setCommitMessage(event.currentTarget.value);
          setErrorMessage(null);
          setStatusMessage(null);
        }}
        onKeyDown={(event) => {
          if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
            event.preventDefault();
            void runCommitAction(false);
          }
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
      {statusMessage ? (
        <p className="conversation-session-commit-status">{statusMessage}</p>
      ) : null}
      {errorMessage ? (
        <p className="conversation-session-commit-status error">{errorMessage}</p>
      ) : null}
      <div className="conversation-session-commit-actions">
        <button
          className="conversation-session-commit-action primary"
          disabled={controlsDisabled || !hasIncludedChanges || !hasValidCommitTarget}
          onClick={() => void runCommitAction(false)}
          type="button"
        >
          {busyAction === "commit" || busyAction === "generate" ? <LoaderCircle className="spin" size={15} /> : <GitCommitHorizontal size={15} />}
          <span>{busyAction === "generate" ? t("gitCommitDialog.generating") : t("gitCommitDialog.commit")}</span>
          <kbd>Ctrl+↵</kbd>
        </button>
        <button
          className="conversation-session-commit-action"
          disabled={controlsDisabled || !hasIncludedChanges || !hasValidCommitTarget}
          onClick={() => void runCommitAction(true)}
          type="button"
        >
          {busyAction === "commitPush" ? <LoaderCircle className="spin" size={15} /> : <CloudUpload size={15} />}
          <span>{busyAction === "commitPush" ? t("gitCommitDialog.running") : t("gitCommitDialog.commitAndPush")}</span>
        </button>
        <button
          className="conversation-session-commit-action"
          disabled={controlsDisabled}
          onClick={() => void runPushAction()}
          type="button"
        >
          {busyAction === "push" ? <LoaderCircle className="spin" size={15} /> : <CloudUpload size={15} />}
          <span>{busyAction === "push" ? t("gitCommitDialog.pushing") : t("gitCommitDialog.push")}</span>
        </button>
      </div>
    </div>
  );
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
  return (
    <section className="conversation-session-tool-section conversation-session-progress-section">
      <div className={cn("conversation-session-tool-section-title", props.onCollapse && "with-action")}>
        <span className="conversation-session-tool-section-title-main">
          <span>进程</span>
          <strong>{props.todolist.completedCount}/{props.todolist.totalCount}</strong>
        </span>
        {props.onCollapse ? (
          <button
            aria-label="折叠会话工具"
            className="workspace-tool-panel-menu-icon-button"
            onClick={props.onCollapse}
            title="折叠会话工具"
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
