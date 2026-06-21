import React from "react";
import {
  CheckCircle2,
  Circle,
  CircleStop,
  ClipboardList,
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
  type TerminalSessionViewModel,
} from "@/components/conversation/conversation-terminal-model";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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
  // Combined uncommitted added line count.
  // 未提交变更合计新增行数。
  additions: number;
  // Combined uncommitted deleted line count.
  // 未提交变更合计删除行数。
  deletions: number;
  // Combined uncommitted changed file count.
  // 未提交变更合计文件数。
  changedFiles: number;
}

// ConversationSessionTodolistPanel renders the session todolist inside the main conversation surface.
// ConversationSessionTodolistPanel 在主会话界面内渲染当前会话的 todolist。
export function ConversationSessionTodolistPanel(props: {
  // Current todolist summary projected from the selected session runtime state.
  // 从当前选中会话运行时状态投影出的 todolist摘要。
  todolist: SessionTodolistSummary | null;
  // Current Git review summary projected from the selected workspace.
  // 从当前选中工作区投影出的 Git 审查摘要。
  reviewSummary: ConversationSessionReviewSummary | null;
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
          reviewSummary={props.reviewSummary}
          onCollapse={() => requestExpandedChange(false)}
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
  const handleStop = React.useCallback(async () => {
    if (!props.session.canStop || stopping) {
      return;
    }
    if (!window.confirm(`确认强制终止终端会话？\n\n${props.session.command}`)) {
      return;
    }
    setStopping(true);
    try {
      await props.onStopCommandSession(props.session.id, props.summary?.ownerSessionId ?? null);
    } finally {
      setStopping(false);
    }
  }, [props, stopping]);
  const tooltipText = terminalSessionTooltipText(props.session, props.summary);
  return (
    <div className="workspace-tool-panel-menu-row conversation-session-terminal-row">
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="conversation-session-terminal-summary">
            <TerminalSquare className={cn("terminal-session-icon", terminalStateClass(props.session.state))} size={15} />
            <strong>{props.session.label}</strong>
            <span className="conversation-session-terminal-name">{props.session.name}</span>
            {props.session.preview ? <small>{props.session.preview}</small> : null}
          </div>
        </TooltipTrigger>
        <TooltipContent className="conversation-session-terminal-tooltip" side="left" sideOffset={8}>
          <pre>{tooltipText}</pre>
        </TooltipContent>
      </Tooltip>
      <button
        aria-label="强制终止终端会话"
        className="conversation-session-terminal-stop"
        disabled={!props.session.canStop || stopping}
        onClick={handleStop}
        title={props.session.canStop ? "强制终止" : props.session.label}
        type="button"
      >
        <CircleStop size={13} />
      </button>
    </div>
  );
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
  // Review summary rendered by this section.
  // 当前区域渲染的审查摘要。
  reviewSummary: ConversationSessionReviewSummary;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Collapses the expanded session tool panel.
  // 折叠当前展开的会话工具面板。
  onCollapse: () => void;
}) {
  return (
    <section className="conversation-session-tool-section conversation-session-review-section">
      <div className="conversation-session-tool-section-title with-action">
        <span>GIT 工具</span>
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
          <span>更改</span>
          {conversationSessionReviewDelta(props.reviewSummary)}
        </button>
        {props.reviewSummary.branchName ? (
          <div className="workspace-tool-panel-menu-row conversation-session-review-row">
            <GitBranch size={15} />
            <span>{conversationSessionBranchDisplayName(props.reviewSummary.branchName)}</span>
          </div>
        ) : null}
        <div className="workspace-tool-panel-menu-row conversation-session-review-row">
          <GitCommitHorizontal size={15} />
          <span>提交</span>
        </div>
      </div>
    </section>
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
