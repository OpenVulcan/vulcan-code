import React from "react";
import {
  ArrowDown,
  ArrowUp,
  Bot,
  Check,
  ChevronDown,
  ChevronRight,
  ClipboardList,
  CloudDownload,
  FileDiff,
  FolderGit2,
  GitBranch,
  GitCommitHorizontal,
  LoaderCircle,
  Maximize2,
  Minimize2,
  Settings2,
  TerminalSquare,
} from "lucide-react";
import {
  terminalSessionViewModel,
  terminalStateTone,
} from "@/components/conversation/conversation-terminal-model";
import {
  ConversationSessionBranchMenu,
  ConversationSessionGitGraphButton,
  type ConversationSessionGitBranchActions,
} from "@/components/conversation/session-todolist-panel/git-branch-controls";
import { ConversationSessionTerminalSection } from "@/components/conversation/session-todolist-panel/terminal-section";
import { toast } from "@/ui-kit/primitives/toast";
import {
  type GitPullActionResponse,
} from "@/service/conversation-actions-service";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog";
import { formatMessage, useI18n } from "@/i18n";
import type { CommandSessionSummary, SessionTodolistItemSummary, SessionTodolistSummary, WorkbenchSnapshot } from "@/types";
import type { ConversationSessionCommitDialogProps } from "@/components/conversation/session-todolist-panel/commit-dialog";

// Conversation session tool morph animation duration in milliseconds.
// 会话工具形态切换动画的毫秒时长。
const CONVERSATION_SESSION_TOOL_TRANSITION_MS = 320;

// REVIEW_LINE_COUNT_FORMAT groups thousands so large line deltas stay scannable.
// REVIEW_LINE_COUNT_FORMAT 为千位分组，使较大的行数增减易于扫读。
const REVIEW_LINE_COUNT_FORMAT = new Intl.NumberFormat("en-US");

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

// ConversationSessionReviewRepository describes one independently actionable Git node.
// ConversationSessionReviewRepository 描述一个可独立操作的 Git 节点。
export interface ConversationSessionReviewRepository {
  // Stable project-relative repository key.
  // 稳定的项目相对仓库键。
  repositoryKey: string;
  // Whether this repository is the deterministic primary repository for the project.
  // 该仓库是否是项目确定性的主要仓库。
  isPrimary: boolean;
  // Compact repository name.
  // 紧凑展示的仓库名称。
  repositoryName: string;
  // Secondary relative repository path.
  // 次级展示的仓库相对路径。
  relativePath: string;
  // Review summary for this repository only.
  // 仅属于该仓库的审查摘要。
  summary: ConversationSessionReviewSummary | null;
  // Whether live review monitoring is enabled.
  // 是否启用实时审查监听。
  monitoringEnabled: boolean;
  // Whether this repository is refreshing.
  // 该仓库是否正在刷新。
  loading: boolean;
  // Latest repository refresh error.
  // 最近一次仓库刷新错误。
  error: string | null;
}

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

// normalizeReviewRepositoryOrderKeys removes stale and duplicate persisted keys while retaining the current repository order.
// normalizeReviewRepositoryOrderKeys 清理持久化顺序中的过期与重复键，同时保留当前仓库顺序。
//
// Parameters
// 参数
// `storedKeys` contains the previously persisted display order.
// `storedKeys` 包含之前持久化的显示顺序。
// `repositoryKeys` contains the repositories currently discovered by the backend.
// `repositoryKeys` 包含后端当前发现的仓库键。
//
// Returns
// 返回值
// Returns each current repository key at most once, with new keys appended deterministically.
// 返回每个当前仓库键最多一次，并将新增键按当前发现顺序追加到末尾。
function normalizeReviewRepositoryOrderKeys(
  storedKeys: readonly string[],
  repositoryKeys: readonly string[],
): string[] {
  const currentKeys = new Set(repositoryKeys);
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const key of storedKeys) {
    if (currentKeys.has(key) && !seen.has(key)) {
      seen.add(key);
      normalized.push(key);
    }
  }
  for (const key of repositoryKeys) {
    if (!seen.has(key)) {
      seen.add(key);
      normalized.push(key);
    }
  }
  return normalized;
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
  // Independent repository review nodes shown for multi-repository projects.
  // 多仓库项目展示的独立仓库审查节点。
  reviewRepositories?: ConversationSessionReviewRepository[];
  // Stable project key used to retain the local repository display order.
  // 用于保留仓库显示顺序的稳定项目键。
  reviewRepositoryOrderStorageKey?: string | null;
  // Whether the shared Git commit dialog is currently open.
  // 共享 Git 提交弹窗当前是否打开。
  commitDialogOpen: boolean;
  // Stable repository key whose commit dialog is currently active.
  // 当前激活提交弹窗对应的稳定仓库键。
  activeReviewRepositoryKey?: string | null;
  // Current shared Git action phase shown by every commit trigger.
  // 每个提交入口展示的当前共享 Git 动作阶段。
  commitBusyState: ConversationSessionCommitBusyState;
  // Resolves the independent Git action phase for one repository node.
  // 解析单个仓库节点独立的 Git 操作阶段。
  commitBusyStateForRepository?: (repositoryKey: string) => ConversationSessionCommitBusyState;
  // Branch switching and history IO bound to the selected workspace; absent renders a read-only branch row.
  // 绑定到当前工作区的分支切换与历史读写；缺省时渲染只读分支行。
  gitBranchActions?: ConversationSessionGitBranchActions | null;
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
  onOpenReviewChanges: (repositoryKey?: string) => void;
  // Pulls one repository (null for the selected one) while showing busyLabel; absent hides the pull choice.
  // 拉取单个仓库（null 表示当前选中仓库），期间显示 busyLabel；缺省时隐藏拉取选项。
  onPullRepository?: (repositoryKey: string | null, busyLabel: string) => Promise<GitPullActionResponse | null>;
  // Selects the repository whose commit dialog is currently active.
  // 选择当前提交弹窗对应的仓库。
  onSelectReviewRepository?: (repositoryKey: string) => void;
  // Persists one repository's monitoring switch.
  // 持久化单个仓库的监听开关。
  onToggleReviewMonitoring?: (repositoryKey: string, enabled: boolean) => void | Promise<void>;
  // Persists the user-selected repository display order.
  // 持久化用户选择的仓库显示顺序。
  onReviewRepositoryOrderChange?: (repositoryKeys: string[]) => void | Promise<void>;
  // Force-stops one live terminal session.
  // 强制停止单条实时终端会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Running background commands of the selected conversation.
  // 当前选中会话中运行中的后台命令数。
  backgroundJobCount?: number;
  // Opens one page of the session activity panel.
  // 打开会话活动面板的一个页面。
  onOpenActivityPanel: (page: "subagents" | "commands") => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const reviewRepositoryLayoutKey = props.reviewRepositories
    ?.map((repository) => `${repository.repositoryKey}:${repository.isPrimary ? "primary" : "secondary"}`)
    .join("\u0000") ?? "";
  const [reviewRepositoryOrderKeys, setReviewRepositoryOrderKeys] = React.useState<string[]>([]);
  const [monitoringTogglePendingKeys, setMonitoringTogglePendingKeys] = React.useState<Set<string>>(new Set());
  const reviewRepositoryOrderStorageKey = props.reviewRepositoryOrderStorageKey
    ? `vulcan.git-review.repository-order.${props.reviewRepositoryOrderStorageKey}`
    : null;
  // Monitoring requests belong to one workspace lifetime, even when another workspace reuses the same repo key.
  // 监听请求属于一个工作区周期，即使另一工作区复用相同仓库键也不会串扰。
  // Each project visit owns synchronous repository locks; an older visit cannot unlock a newer request.
  // 每次项目访问持有同步仓库锁；较早访问不能解锁较新请求。
  const monitoringScope = React.useMemo(() => new Set<string>(), [reviewRepositoryOrderStorageKey]);
  const monitoringScopeRef = React.useRef(monitoringScope);
  monitoringScopeRef.current = monitoringScope;
  React.useEffect(() => setMonitoringTogglePendingKeys(new Set()), [monitoringScope]);
  React.useEffect(() => {
    if (!reviewRepositoryOrderStorageKey) {
      setReviewRepositoryOrderKeys([]);
      return;
    }
    try {
      const stored = window.localStorage.getItem(reviewRepositoryOrderStorageKey);
      const parsed = stored ? JSON.parse(stored) : null;
      setReviewRepositoryOrderKeys(
        Array.isArray(parsed) && parsed.every((key): key is string => typeof key === "string") ? parsed : [],
      );
    } catch {
      setReviewRepositoryOrderKeys([]);
    }
  }, [reviewRepositoryOrderStorageKey]);
  React.useEffect(() => {
    const repositoryKeys = (props.reviewRepositories ?? []).map((repository) => repository.repositoryKey);
    setReviewRepositoryOrderKeys((current) => {
      const next = normalizeReviewRepositoryOrderKeys(current, repositoryKeys);
      return next.length === current.length && next.every((key, index) => key === current[index]) ? current : next;
    });
  }, [reviewRepositoryLayoutKey]);
  const orderedReviewRepositories = React.useMemo(() => {
    const repositoriesByKey = new Map((props.reviewRepositories ?? []).map((repository) => [repository.repositoryKey, repository]));
    const ordered = reviewRepositoryOrderKeys
      .map((repositoryKey) => repositoriesByKey.get(repositoryKey))
      .filter((repository): repository is ConversationSessionReviewRepository => Boolean(repository));
    for (const repository of props.reviewRepositories ?? []) {
      if (!ordered.some((item) => item.repositoryKey === repository.repositoryKey)) {
        ordered.push(repository);
      }
    }
    return ordered;
  }, [props.reviewRepositories, reviewRepositoryOrderKeys]);
  const visibleReviewRepositories = React.useMemo(
    () => orderedReviewRepositories.filter((repository) => repository.monitoringEnabled),
    [orderedReviewRepositories],
  );
  // multipleReviewRepositories turns each monitored repository into a foldable group.
  // multipleReviewRepositories 使每个已监听仓库成为可折叠分组。
  const multipleReviewRepositories = visibleReviewRepositories.length > 1;
  // repositoryExpansionOverrides records groups the user explicitly opened or folded; the rest follow the default.
  // repositoryExpansionOverrides 记录用户显式展开或折叠的分组；其余分组遵循默认状态。
  const [repositoryExpansionOverrides, setRepositoryExpansionOverrides] = React.useState<ReadonlyMap<string, boolean>>(new Map());
  React.useEffect(() => setRepositoryExpansionOverrides(new Map()), [reviewRepositoryOrderStorageKey]);
  // Only the first group in display order starts expanded, so several repositories stay compact by default.
  // 默认仅展开显示顺序中的首个分组，使多仓库面板保持紧凑。
  const defaultExpandedRepositoryKey = visibleReviewRepositories[0]?.repositoryKey ?? null;
  const isReviewRepositoryExpanded = (repositoryKey: string) =>
    repositoryExpansionOverrides.get(repositoryKey) ?? repositoryKey === defaultExpandedRepositoryKey;
  const setReviewRepositoryExpanded = React.useCallback((repositoryKey: string, expanded: boolean) => {
    setRepositoryExpansionOverrides((current) => new Map(current).set(repositoryKey, expanded));
  }, []);
  React.useEffect(() => {
    setMonitoringTogglePendingKeys((current) => {
      const repositoryKeys = new Set((props.reviewRepositories ?? []).map((repository) => repository.repositoryKey));
      const next = new Set([...current].filter((key) => repositoryKeys.has(key)));
      return next.size === current.size ? current : next;
    });
  }, [reviewRepositoryLayoutKey]);
  // toggleReviewMonitoring serializes one repository preference click until its persistence request settles.
  // toggleReviewMonitoring 在偏好持久化请求结束前串行化单个仓库的开关点击。
  // Parameters: repositoryKey is the scoped node and enabled is the requested preference. Returns after the host settles and its own lock releases.
  // 参数：repositoryKey 是作用域内节点，enabled 是请求的偏好。宿主结束且自身锁释放后返回。
  const toggleReviewMonitoring = React.useCallback(async (repositoryKey: string, enabled: boolean) => {
    if (!props.onToggleReviewMonitoring || monitoringScope.has(repositoryKey)
      || props.commitBusyStateForRepository?.(repositoryKey)) {
      return;
    }
    monitoringScope.add(repositoryKey);
    setMonitoringTogglePendingKeys((current) => {
      const next = new Set(current);
      next.add(repositoryKey);
      return next;
    });
    try {
      await props.onToggleReviewMonitoring(repositoryKey, enabled);
    } finally {
      monitoringScope.delete(repositoryKey);
      if (monitoringScopeRef.current === monitoringScope) {
        setMonitoringTogglePendingKeys((current) => {
          const next = new Set(current);
          next.delete(repositoryKey);
          return next;
        });
      }
    }
  }, [monitoringScope, props.commitBusyStateForRepository, props.onToggleReviewMonitoring]);
  const moveReviewRepository = React.useCallback((repositoryKey: string, direction: -1 | 1) => {
    const orderedKeys = normalizeReviewRepositoryOrderKeys(
      reviewRepositoryOrderKeys,
      orderedReviewRepositories.map((repository) => repository.repositoryKey),
    );
    const index = orderedKeys.indexOf(repositoryKey);
    const targetIndex = index + direction;
    if (index < 0 || targetIndex < 0 || targetIndex >= orderedKeys.length) {
      return;
    }
    const next = [...orderedKeys];
    [next[index], next[targetIndex]] = [next[targetIndex], next[index]];
    setReviewRepositoryOrderKeys(next);
    if (reviewRepositoryOrderStorageKey) {
      try {
        window.localStorage.setItem(reviewRepositoryOrderStorageKey, JSON.stringify(next));
      } catch {
        // Local storage is optional; the in-memory order remains authoritative for this panel.
        // 本地存储是可选能力；当前面板仍以内存顺序为准。
      }
    }
    void props.onReviewRepositoryOrderChange?.(next);
  }, [orderedReviewRepositories, props.onReviewRepositoryOrderChange, reviewRepositoryOrderKeys, reviewRepositoryOrderStorageKey]);
  const terminalSessions = React.useMemo(
    () => props.terminalSessions
      .map((summary) => terminalSessionViewModel(summary, t))
      .filter((session) => session.canStop),
    [props.terminalSessions, t],
  );
  const terminalSummaryById = React.useMemo(
    () => new Map(props.terminalSessions.map((summary) => [summary.id, summary])),
    [props.terminalSessions],
  );
  const hasTerminalSessions = terminalSessions.length > 0;
  // subagentTasks mirrors the backend navigation rows retained for the current parent turn.
  // subagentTasks 镜像后端为当前父级回合保留的导航行。
  const subagentTasks = React.useMemo(
    () => props.subagentNavigation?.tasks ?? [],
    [props.subagentNavigation?.tasks],
  );
  const hasSubagentTasks = subagentTasks.length > 0;
  const backgroundJobCount = props.backgroundJobCount ?? 0;
  // showReviewRepositoryManager keeps disabled repositories reachable without rendering them as panel nodes.
  // showReviewRepositoryManager 让已关闭仓库仍可从设置进入，同时不把它们渲染为面板节点。
  const showReviewRepositoryManager = orderedReviewRepositories.length > 1
    || (orderedReviewRepositories.length > 0 && visibleReviewRepositories.length === 0);
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

  // morph marks the surface while it plays the short expand or collapse fade.
  // morph 在表面播放短暂的展开或折叠淡入时进行标记。
  const morph = transitionPhase ?? undefined;

  if (!renderExpanded) {
    return (
      <button
        aria-label={t("sessionTools.expand")}
        className="vk-stools"
        data-collapsed=""
        data-morph={morph}
        onClick={() => requestExpandedChange(true)}
        title={t("sessionTools.expand")}
        type="button"
      >
        {props.reviewSummary ? (
          <span className="vk-stools-row">
            <FileDiff size={14} />
            <span>{t("sessionTools.changes")}</span>
            {conversationSessionReviewDelta(props.reviewSummary)}
          </span>
        ) : null}
        {!props.reviewSummary && props.reviewRepositories?.length ? (
          <span className="vk-stools-row">
            <GitBranch size={14} />
            <span>{t("gitCommitDialog.monitoringManager")}</span>
            <b>{visibleReviewRepositories.length}</b>
          </span>
        ) : null}
        {props.todolist ? (
          <span className="vk-stools-row">
            <ClipboardList size={14} />
            <span>{t("sessionTools.progress")}</span>
            <b>{props.todolist.completedCount}/{props.todolist.totalCount}</b>
          </span>
        ) : null}
        {hasTerminalSessions ? (
          <span className="vk-stools-row">
            <TerminalSquare
              className="vk-term-mark"
              data-state={terminalSessions[0].state}
              data-tone={terminalStateTone(terminalSessions[0].state)}
              size={14}
            />
            <span>{t("sessionTools.terminal")}</span>
            <b>{terminalSessions.length}</b>
          </span>
        ) : null}
        {hasSubagentTasks ? (
          <span className="vk-stools-row">
            <Bot size={14} />
            <span>{t("conversation.subagent.schedule")}</span>
            <b>{subagentTasks.length}</b>
          </span>
        ) : null}
        {/* The background command count stays visible, even at 0, as the way into the activity panel. */}
        {/* 后台命令数始终可见（包括 0），作为进入活动面板的入口。 */}
        <span className="vk-stools-row">
          <TerminalSquare size={14} />
          <span>{t("sessionActivity.backgroundCommands")}</span>
          <b>{backgroundJobCount}</b>
        </span>
        <Maximize2 className="vk-stools-hint" size={12} />
      </button>
    );
  }

  return (
    <aside className="vk-stools" data-morph={morph} aria-label={t("sessionTools.menu")}>
      <div className="vk-stools-head">
        <span className="vk-label">{t("sessionTools.title")}</span>
        {showReviewRepositoryManager ? (
          <ConversationSessionReviewMonitoringManager
            repositories={orderedReviewRepositories}
            pendingRepositoryKeys={new Set([...monitoringTogglePendingKeys,
              ...orderedReviewRepositories.filter((repository) => props.commitBusyStateForRepository?.(repository.repositoryKey))
                .map((repository) => repository.repositoryKey)])}
            onMoveRepository={moveReviewRepository}
            onToggleMonitoring={toggleReviewMonitoring}
          />
        ) : null}
        <IconButton
          aria-label={t("sessionTools.collapse")}
          onClick={() => requestExpandedChange(false)}
          size="xs"
          title={t("sessionTools.collapse")}
        >
          <Minimize2 size={13} />
        </IconButton>
      </div>
      <div className="vk-stools-scroll">
      {visibleReviewRepositories.map((repository) => (
        <ConversationSessionReviewSection
          key={repository.repositoryKey}
          commitBusyState={props.commitBusyStateForRepository
            ? props.commitBusyStateForRepository(repository.repositoryKey)
            : props.activeReviewRepositoryKey === repository.repositoryKey ? props.commitBusyState : null}
          commitDialogOpen={props.commitDialogOpen && props.activeReviewRepositoryKey === repository.repositoryKey}
          error={repository.error}
          gitBranchActions={props.gitBranchActions ?? null}
          expanded={multipleReviewRepositories ? isReviewRepositoryExpanded(repository.repositoryKey) : undefined}
          loading={repository.loading}
          relativePath={repository.relativePath}
          repositoryKey={repository.repositoryKey}
          repositoryName={repository.repositoryName}
          reviewSummary={repository.summary}
          onCommitDialogOpenChange={(open) => {
            props.onSelectReviewRepository?.(repository.repositoryKey);
            props.onCommitDialogOpenChange(open);
          }}
          onExpandedChange={multipleReviewRepositories
            ? (expanded) => setReviewRepositoryExpanded(repository.repositoryKey, expanded)
            : undefined}
          onOpenReviewChanges={() => props.onOpenReviewChanges(repository.repositoryKey)}
          onPull={props.onPullRepository
            ? (busyLabel) => props.onPullRepository!(repository.repositoryKey, busyLabel)
            : undefined}
        />
      ))}
      {!props.reviewRepositories?.length && props.reviewSummary ? (
        <ConversationSessionReviewSection
          commitBusyState={props.commitBusyState}
          commitDialogOpen={props.commitDialogOpen}
          gitBranchActions={props.gitBranchActions ?? null}
          reviewSummary={props.reviewSummary}
          onCommitDialogOpenChange={props.onCommitDialogOpenChange}
          onOpenReviewChanges={() => props.onOpenReviewChanges()}
          onPull={props.onPullRepository ? (busyLabel) => props.onPullRepository!(null, busyLabel) : undefined}
        />
      ) : null}
      {hasTerminalSessions ? (
        <ConversationSessionTerminalSection
          summariesById={terminalSummaryById}
          terminalSessions={terminalSessions}
          onStopCommandSession={props.onStopCommandSession}
        />
      ) : null}
      {props.todolist ? (
        <ConversationSessionProgressSection
          todolist={props.todolist}
        />
      ) : null}
      <ConversationSessionActivitySection
        backgroundJobCount={backgroundJobCount}
        tasks={subagentTasks}
        onOpenActivityPanel={props.onOpenActivityPanel}
      />
      </div>
    </aside>
  );
}

// ConversationSessionReviewMonitoringManager edits the enabled repository set and display order.
// ConversationSessionReviewMonitoringManager 编辑监听仓库集合与显示顺序。
function ConversationSessionReviewMonitoringManager(props: {
  // Repositories in their current display order.
  // 当前显示顺序下的仓库集合。
  repositories: ConversationSessionReviewRepository[];
  // Repository keys currently waiting for a persistence request.
  // 正在等待偏好持久化请求的仓库键集合。
  pendingRepositoryKeys: ReadonlySet<string>;
  // Moves one repository one slot in the display order.
  // 将一个仓库在显示顺序中移动一个位置。
  onMoveRepository: (repositoryKey: string, direction: -1 | 1) => void;
  // Persists one repository monitoring switch.
  // 持久化单个仓库的监听开关。
  onToggleMonitoring: (repositoryKey: string, enabled: boolean) => void | Promise<void>;
}) {
  const { t } = useI18n();
  const [open, setOpen] = React.useState(false);
  return (
    <>
      <IconButton
        aria-label={t("gitCommitDialog.monitoringManager")}
        className="vk-stools-manage"
        onClick={() => setOpen(true)}
        size="xs"
        title={t("gitCommitDialog.monitoringManager")}
      >
        <Settings2 size={13} />
      </IconButton>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent size="sm">
          <DialogHeader>
            <DialogTitle>{t("gitCommitDialog.monitoringManager")}</DialogTitle>
            <DialogDescription>{t("gitCommitDialog.monitoringManagerDescription")}</DialogDescription>
          </DialogHeader>
          <div className="vk-repo-list">
            {props.repositories.map((repository, index) => {
              const pending = props.pendingRepositoryKeys.has(repository.repositoryKey);
              return (
                <div className="vk-repo-row" data-repository-key={repository.repositoryKey} key={repository.repositoryKey}>
                  <label>
                    <Checkbox
                      checked={repository.monitoringEnabled}
                      disabled={pending}
                      onCheckedChange={(checked) => void props.onToggleMonitoring(repository.repositoryKey, checked === true)}
                    />
                    <span>
                      <strong>{repository.repositoryName}</strong>
                      {repository.relativePath && repository.relativePath !== "." && repository.relativePath !== repository.repositoryName ? (
                        <small>{repository.relativePath}</small>
                      ) : null}
                    </span>
                  </label>
                  <div>
                    <IconButton
                      aria-label={t("gitCommitDialog.moveRepositoryUp")}
                      disabled={index === 0}
                      onClick={() => props.onMoveRepository(repository.repositoryKey, -1)}
                      size="xs"
                      title={t("gitCommitDialog.moveRepositoryUp")}
                    >
                      <ArrowUp size={13} />
                    </IconButton>
                    <IconButton
                      aria-label={t("gitCommitDialog.moveRepositoryDown")}
                      disabled={index === props.repositories.length - 1}
                      onClick={() => props.onMoveRepository(repository.repositoryKey, 1)}
                      size="xs"
                      title={t("gitCommitDialog.moveRepositoryDown")}
                    >
                      <ArrowDown size={13} />
                    </IconButton>
                  </div>
                </div>
              );
            })}
          </div>
        </DialogContent>
      </Dialog>
    </>
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
  reviewSummary: ConversationSessionReviewSummary | null;
  // Optional stable repository key rendered in a multi-repository node.
  // 多仓库节点中展示的可选稳定仓库键。
  repositoryKey?: string;
  // Optional repository display name.
  // 可选仓库展示名称。
  repositoryName?: string;
  // Optional relative path, shown only as the name tooltip.
  // 可选相对路径，仅作为名称提示展示。
  relativePath?: string;
  // Whether this repository node is refreshing.
  // 该仓库节点是否正在刷新。
  loading?: boolean;
  // Whether this repository group body is expanded; only meaningful together with onExpandedChange.
  // 该仓库分组正文是否展开；仅在提供 onExpandedChange 时有意义。
  expanded?: boolean;
  // Latest repository refresh error.
  // 该仓库节点最近一次刷新错误。
  error?: string | null;
  // Branch switching and history IO; null renders a read-only branch row.
  // 分支切换与历史读写；为 null 时渲染只读分支行。
  gitBranchActions: ConversationSessionGitBranchActions | null;
  // Opens the workspace review panel focused on uncommitted changes.
  // 打开工作区审核面板并聚焦未提交变更。
  onOpenReviewChanges: () => void;
  // Folds or unfolds this repository group; absent for a single repository, which never folds.
  // 折叠或展开该仓库分组；单仓库不折叠，因此不提供。
  onExpandedChange?: (expanded: boolean) => void;
  // Opens or closes the shared Git commit dialog.
  // 打开或关闭共享 Git 提交弹窗。
  onCommitDialogOpenChange: (open: boolean) => void;
  // Pulls this repository while showing busyLabel; absent hides the split menu.
  // 拉取该仓库，期间显示 busyLabel；缺省时隐藏分割菜单。
  onPull?: (busyLabel: string) => Promise<GitPullActionResponse | null>;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // hasGitCommitOrPushContent tracks whether the compact Git entry can open a meaningful action.
  // hasGitCommitOrPushContent 判断紧凑 Git 入口是否存在可执行的提交或推送内容。
  const hasGitCommitOrPushContent = props.reviewSummary !== null && (props.reviewSummary.stagedChangedFiles > 0
    || props.reviewSummary.unstagedChangedFiles > 0
    || props.reviewSummary.unpushedCommits > 0);
  // foldable marks a multi-repository group whose body can be folded down to its title row.
  // foldable 标记可折叠到标题行的多仓库分组。
  const foldable = Boolean(props.onExpandedChange);
  const reviewSectionExpanded = !foldable || props.expanded !== false;
  // commitTriggerDisabled disables the entry while Git is busy or no commit/push action is possible.
  // commitTriggerDisabled 在 Git 忙碌或没有可提交/可推送内容时禁用入口。
  const gitActionBlocked = props.loading || Boolean(props.error) || Boolean(props.commitBusyState);
  const commitTriggerDisabled = gitActionBlocked || !hasGitCommitOrPushContent;
  // runPull fetches upstream commits even when nothing is waiting to be committed or pushed.
  // runPull 即使没有待提交或待推送的内容也会拉取上游提交。
  const runPull = async () => {
    if (!props.onPull) return;
    try {
      const response = await props.onPull(t("gitCommitDialog.pulling"));
      if (!response) return;
      toast.success(t(response.updated ? "gitCommitDialog.pullDone" : "gitCommitDialog.pullUpToDate"), {
        description: response.updated && response.commitHash
          ? [`# ${response.commitHash}`, response.commitMessage].filter(Boolean).join("\n")
          : undefined,
      });
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };
  const repositoryName = props.repositoryName ?? t("gitCommitDialog.toolsTitle");
  // The path is only a tooltip: it is noise for the primary repository and the name already identifies nested ones.
  // 路径仅作为提示：对主仓库是噪声，嵌套仓库的名称已足以识别。
  const repositoryTitle = props.relativePath && props.relativePath !== "." ? props.relativePath : undefined;
  // A folded group still surfaces refresh and Git action progress on its title row.
  // 折叠的分组仍在标题行显示刷新与 Git 动作进度。
  const titleBusy = props.loading || (!reviewSectionExpanded && Boolean(props.commitBusyState));
  const titleStatus = (
    <span className="vk-stools-status">
      {titleBusy ? (
        <LoaderCircle aria-label={props.commitBusyState?.label ?? t("gitCommitDialog.running")} className="vk-spin" size={13} />
      ) : null}
      {props.error ? <span aria-label={props.error} className="vk-stools-error" title={props.error}>!</span> : null}
      {!reviewSectionExpanded && props.reviewSummary !== null ? conversationSessionReviewDelta(props.reviewSummary) : null}
    </span>
  );

  // A repository reads as one line of identity (name, branch, how far it leads its remote) over what can be
  // done with it: its changes, then committing or pushing and the commit graph.
  // 仓库以一行身份信息呈现（名称、分支、领先远端多少），其下是可对它执行的操作：变更，然后是提交或推送与提交图谱。
  const unpushed = props.reviewSummary?.unpushedCommits ?? 0;
  return (
    <section
      className="vk-stools-sec"
      data-foldable={foldable ? "" : undefined}
      data-kind="review"
      data-repository-key={props.repositoryKey}
    >
      <div className="vk-stools-title" data-kind="repo">
        {foldable ? (
          <button
            aria-expanded={reviewSectionExpanded}
            className="vk-stools-fold"
            data-action="fold"
            onClick={() => props.onExpandedChange?.(!reviewSectionExpanded)}
            title={repositoryTitle}
            type="button"
          >
            <ChevronRight className="vk-stools-chev" size={13} />
            <span className="vk-stools-name">{repositoryName}</span>
          </button>
        ) : (
          <>
            <FolderGit2 size={13} />
            <span className="vk-stools-name" title={repositoryTitle}>{repositoryName}</span>
          </>
        )}
        {props.reviewSummary?.branchName ? (
          <ConversationSessionBranchMenu
            actions={props.gitBranchActions}
            branchName={props.reviewSummary.branchName}
            busy={Boolean(props.loading) || Boolean(props.commitBusyState)}
            changedFiles={props.reviewSummary.changedFiles}
            repositoryKey={props.repositoryKey ?? null}
          />
        ) : null}
        {unpushed > 0 ? (
          <span className="vk-badge" data-tone="info" title={formatMessage(t("sessionTools.ahead"), { count: unpushed })}>↑{unpushed}</span>
        ) : null}
        {titleStatus}
      </div>
      {reviewSectionExpanded && props.error ? <p className="vk-stools-note" data-tone="bad" role="alert">{props.error}</p> : null}
      {reviewSectionExpanded && props.error ? <button className="vk-stools-item" data-row="changes"
        disabled={props.loading} onClick={props.onOpenReviewChanges} type="button">
        <FileDiff size={14} /><span>{t("gitRepository.retryReview")}</span>
      </button> : null}
      {reviewSectionExpanded && props.reviewSummary === null ? <p className="vk-stools-note">{t(props.loading ? "gitRepository.loading" : "gitRepository.unavailable")}</p> : null}
      {reviewSectionExpanded && props.reviewSummary !== null ? (
        <div className="vk-stools-list">
          <button
            className="vk-stools-item"
            data-row="changes"
            disabled={props.loading || Boolean(props.error)}
            onClick={props.onOpenReviewChanges}
            type="button"
          >
            <FileDiff size={14} />
            <span>{t("gitCommitDialog.changes")}</span>
            <small>{formatMessage(t("sessionTools.files"), { count: props.reviewSummary.changedFiles })}</small>
            {conversationSessionReviewDelta(props.reviewSummary)}
          </button>
          <div className="vk-stools-actions">
            {/* The main part opens commit or push; the arrow lists the other Git actions, which stay reachable */}
            {/* when nothing is waiting to be committed. */}
            {/* 主体打开提交或推送；箭头列出其他 Git 操作，即使没有待提交内容也可使用。 */}
            <span className="vk-stools-split">
              <Button
                aria-expanded={props.commitDialogOpen}
                data-row="commit"
                disabled={commitTriggerDisabled}
                onClick={() => props.onCommitDialogOpenChange(true)}
                size="sm"
                type="button"
                variant="primary"
              >
                {props.commitBusyState ? <LoaderCircle className="vk-spin" /> : <GitCommitHorizontal />}
                {props.commitBusyState?.label ?? t("gitCommitDialog.open")}
              </Button>
              {props.onPull ? (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      aria-label={t("gitCommitDialog.moreActions")}
                      data-part="menu"
                      disabled={gitActionBlocked}
                      size="sm"
                      title={t("gitCommitDialog.moreActions")}
                      type="button"
                      variant="primary"
                    >
                      <ChevronDown />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" sideOffset={4}>
                    <DropdownMenuItem onSelect={() => void runPull()}>
                      <CloudDownload size={14} />
                      <span>{t("gitCommitDialog.pull")}</span>
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              ) : null}
            </span>
            {props.gitBranchActions ? (
              <ConversationSessionGitGraphButton actions={props.gitBranchActions} repositoryKey={props.repositoryKey ?? null} />
            ) : null}
          </div>
        </div>
      ) : null}
    </section>
  );
}

// LazyConversationSessionCommitDialog loads the commit dialog module on first open.
// LazyConversationSessionCommitDialog 在首次打开时加载提交弹窗模块。
const LazyConversationSessionCommitDialog = React.lazy(async () => ({
  default: (await import("@/components/conversation/session-todolist-panel/commit-dialog")).ConversationSessionCommitDialog,
}));

// ConversationSessionCommitDialog renders the shared Git commit or push dialog once it has been opened; it stays mounted
// afterwards so closing keeps its transition.
// ConversationSessionCommitDialog 在首次打开后渲染共享 Git 提交或推送弹窗；此后保持挂载，使关闭保留过渡效果。
export function ConversationSessionCommitDialog(props: ConversationSessionCommitDialogProps) {
  const [opened, setOpened] = React.useState(props.open);
  if (props.open && !opened) setOpened(true);
  if (!opened) return null;
  return <React.Suspense fallback={null}><LazyConversationSessionCommitDialog {...props} /></React.Suspense>;
}

// ConversationSessionActivitySection shows only the subagent and background command counts; each row opens its page
// of the activity panel, which holds the lists and the records of ended work. Both rows stay visible at 0, so the panel
// is always reachable.
// ConversationSessionActivitySection 只显示子代理与后台命令的数量；每行打开活动面板中对应的页面，列表及已结束的记录
// 在该面板中。两行在数量为 0 时也保持可见，使活动面板始终可以打开。
function ConversationSessionActivitySection(props: {
  // Subagent rows of the parent conversation.
  // 父会话的子代理行。
  tasks: ConversationSessionSubagentTask[];
  // Running background commands.
  // 运行中的后台命令数。
  backgroundJobCount: number;
  // Opens one page of the activity panel.
  // 打开活动面板的一个页面。
  onOpenActivityPanel: (page: "subagents" | "commands") => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const runningTasks = props.tasks.filter((task) => task.isRunning).length;
  return (
    <section className="vk-stools-sec" data-kind="activity">
      <div className="vk-stools-list">
        <button
          className="vk-stools-item"
          data-row="subagents"
          onClick={() => props.onOpenActivityPanel("subagents")}
          type="button"
        >
          <Bot size={14} />
          <span>{t("conversation.subagent.schedule")}</span>
          <small data-tone={runningTasks > 0 ? "ok" : undefined}>
            {formatMessage(t("sessionActivity.runningOfTotal"), { running: runningTasks, total: props.tasks.length })}
          </small>
        </button>
        <button
          className="vk-stools-item"
          data-row="commands"
          onClick={() => props.onOpenActivityPanel("commands")}
          type="button"
        >
          <TerminalSquare size={14} />
          <span>{t("sessionActivity.backgroundCommands")}</span>
          <small data-tone={props.backgroundJobCount > 0 ? "ok" : undefined}>{props.backgroundJobCount}</small>
        </button>
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
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <section className="vk-stools-sec" data-kind="progress">
      <div className="vk-stools-title">
        <ClipboardList size={13} />
        <span>{t("sessionTools.progress")}</span>
        <b>{props.todolist.completedCount}/{props.todolist.totalCount}</b>
      </div>
      <ol className="vk-stools-steps">
        {props.todolist.items.map((item, index) => (
          <ConversationSessionTodolistRow item={item} key={`${index}-${item.status}-${item.step}`} />
        ))}
      </ol>
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
    <li data-state={props.item.status === "completed" ? "done" : props.item.status === "in_progress" ? "active" : undefined}>
      <i>{conversationSessionTodolistStatusIcon(props.item.status)}</i>
      <span>{props.item.step}</span>
    </li>
  );
}

// conversationSessionTodolistStatusIcon returns the compact icon for one todolist status.
// conversationSessionTodolistStatusIcon 返回单个 todolist 状态对应的紧凑图标。
function conversationSessionTodolistStatusIcon(status: SessionTodolistItemSummary["status"]): React.ReactNode {
  if (status === "completed") {
    return <Check strokeWidth={3} />;
  }
  if (status === "in_progress") {
    return <LoaderCircle className="vk-spin" />;
  }
  return null;
}

// conversationSessionReviewDelta renders colored added and deleted line counters.
// conversationSessionReviewDelta 渲染带颜色的新增与删除行数。
function conversationSessionReviewDelta(reviewSummary: ConversationSessionReviewSummary): React.ReactNode {
  return (
    <b className="vk-stools-delta">
      <span className="vk-add">+{REVIEW_LINE_COUNT_FORMAT.format(reviewSummary.additions)}</span>
      <span className="vk-del">-{REVIEW_LINE_COUNT_FORMAT.format(reviewSummary.deletions)}</span>
    </b>
  );
}
