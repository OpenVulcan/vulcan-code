// Session activity panel: the subagents and the command jobs of one root session, each list on its own page.
// Ended entries before a clear are hidden by default and stay reachable; nothing is deleted.
// 会话活动面板：一个根会话的子代理与命令任务，每个列表一页。清理之前已结束的条目默认隐藏且仍可查看；不删除任何内容。
import "@/ui-kit/styles/features/session-activity.css";
import * as React from "react";
import { Bot, ChevronDown, ChevronRight, Eraser, Square, TerminalSquare } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import { toast } from "@/ui-kit/primitives/toast";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { serviceClient } from "@/service/webui-service-runtime";
import { SHELL_JOB_CHANGED_TOPIC } from "@/service/shell-job-events";
import { listShellJobs } from "@/service/session-activity-service";
import { clearSessionActivity, loadSessionActivity, stopShellJob } from "@/service/session-activity-panel-service";
import type {
  SessionActivitySnapshot,
  ShellJobChangedEvent,
  ShellJobList,
  ShellJobView,
  SubagentActivity,
} from "@/types";
import { ShellJobLogView } from "./shell-job-log-view";
import { formatActivityDuration } from "./session-activity-format";
import {
  conversationSessionSubagentRunStateLabel,
  conversationSessionSubagentRunStateTone,
} from "@/components/conversation/session-subagent-state";

// SessionActivityPage names one page of the panel.
// SessionActivityPage 指明面板的一页。
export type SessionActivityPage = "subagents" | "commands";

// FINISHED_JOB_LIMIT is how many finished command jobs show before the user asks for all of them.
// FINISHED_JOB_LIMIT 是用户要求查看全部之前显示的已结束命令任务数。
const FINISHED_JOB_LIMIT = 5;

// LIVE_REFRESH_MS is how often running entries refresh their recent lines while the page is shown.
// LIVE_REFRESH_MS 是页面显示期间运行中条目刷新最近输出行的间隔。
const LIVE_REFRESH_MS = 2000;

// SessionActivityPanelProps describes the panel boundary.
// SessionActivityPanelProps 描述面板边界。
export interface SessionActivityPanelProps {
  // Root session shown by the panel; null shows an empty state.
  // 面板显示的根会话；为空时显示空状态。
  sessionId: string | null;
  // Page requested by the opener.
  // 打开方请求的页面。
  page: SessionActivityPage;
  // Increases with every open request so the same page can be requested again.
  // 每次打开请求都会递增，使同一页面可再次被请求。
  pageRequest: number;
  // Changes whenever the subagent navigation of the session changes.
  // 会话子代理导航变化时随之变化。
  subagentRevision: string;
  // Opens one subagent transcript tab.
  // 打开一个子代理转录页签。
  onOpenSubagent: (parentConversationId: string, taskId: string) => Promise<boolean>;
}

// SessionActivityPanel renders the two pages with their clear and show-all controls.
// SessionActivityPanel 渲染两个页面及其清理与显示全部控件。
export function SessionActivityPanel(props: SessionActivityPanelProps) {
  const { t } = useI18n();
  const [page, setPage] = React.useState<SessionActivityPage>(props.page);
  React.useEffect(() => setPage(props.page), [props.page, props.pageRequest]);
  const subagents = useSubagentActivity(props.sessionId, page === "subagents", props.subagentRevision);
  const jobs = useShellJobList(props.sessionId, page === "commands");
  if (!props.sessionId) {
    return <p className="vk-act-none">{t("sessionActivity.noSession")}</p>;
  }
  return (
    <div className="vk-act">
      <div className="vk-act-tabs" role="tablist">
        <button aria-selected={page === "subagents"} className="vk-act-tab" onClick={() => setPage("subagents")} role="tab" type="button">
          <Bot size={13} />
          <span>{t("sessionActivity.subagents")}</span>
          {subagents.snapshot ? <b>{subagents.snapshot.subagents.length}</b> : null}
        </button>
        <button aria-selected={page === "commands"} className="vk-act-tab" onClick={() => setPage("commands")} role="tab" type="button">
          <TerminalSquare size={13} />
          <span>{t("sessionActivity.commands")}</span>
          {jobs.list ? <b>{jobs.list.runningCount}</b> : null}
        </button>
      </div>
      {page === "subagents" ? (
        <SubagentActivityPage
          sessionId={props.sessionId}
          state={subagents}
          onOpenSubagent={props.onOpenSubagent}
        />
      ) : (
        <ShellJobPage sessionId={props.sessionId} state={jobs} />
      )}
    </div>
  );
}

// ActivityListState is the shared loading state of one page.
// ActivityListState 是一个页面共享的加载状态。
interface ActivityListState {
  // Whether ended entries before the latest clear are shown.
  // 是否显示最近一次清理之前已结束的条目。
  showAll: boolean;
  // Switches showAll.
  // 切换 showAll。
  setShowAll: (showAll: boolean) => void;
  // Latest load failure.
  // 最近一次加载失败。
  error: string | null;
  // Reloads the list now.
  // 立即重新加载列表。
  reload: () => void;
}

// SubagentActivityState carries the subagent page data.
// SubagentActivityState 承载子代理页面数据。
interface SubagentActivityState extends ActivityListState {
  // Latest snapshot.
  // 最新快照。
  snapshot: SessionActivitySnapshot | null;
}

// ShellJobListState carries the command page data.
// ShellJobListState 承载命令页面数据。
interface ShellJobListState extends ActivityListState {
  // Latest list.
  // 最新列表。
  list: ShellJobList | null;
}

// useSubagentActivity loads the subagent list and refreshes it while a subagent runs and the page is shown.
// useSubagentActivity 加载子代理列表，并在子代理运行且页面显示期间刷新。
function useSubagentActivity(sessionId: string | null, visible: boolean, revision: string): SubagentActivityState {
  const [snapshot, setSnapshot] = React.useState<SessionActivitySnapshot | null>(null);
  const [showAll, setShowAll] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [reloadKey, setReloadKey] = React.useState(0);
  React.useEffect(() => {
    setSnapshot(null);
  }, [sessionId]);
  React.useEffect(() => {
    if (!sessionId) return undefined;
    let cancelled = false;
    let timer: number | null = null;
    const load = async () => {
      try {
        const next = await loadSessionActivity(sessionId, showAll);
        if (cancelled) return;
        setSnapshot(next);
        setError(null);
        if (visible && next?.subagents.some((entry) => entry.task.isRunning)) {
          timer = window.setTimeout(() => void load(), LIVE_REFRESH_MS);
        }
      } catch (loadError) {
        if (!cancelled) setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    };
    void load();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [reloadKey, revision, sessionId, showAll, visible]);
  const reload = React.useCallback(() => setReloadKey((key) => key + 1), []);
  return { error, reload, setShowAll, showAll, snapshot };
}

// useShellJobList loads the command list, follows job notices and refreshes running jobs while the page is shown.
// useShellJobList 加载命令列表，跟随任务通知，并在页面显示期间刷新运行中的任务。
function useShellJobList(sessionId: string | null, visible: boolean): ShellJobListState {
  const [list, setList] = React.useState<ShellJobList | null>(null);
  const [showAll, setShowAll] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [reloadKey, setReloadKey] = React.useState(0);
  React.useEffect(() => {
    setList(null);
  }, [sessionId]);
  React.useEffect(() => {
    if (!sessionId) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    void serviceClient.subscribe<ShellJobChangedEvent>(SHELL_JOB_CHANGED_TOPIC, (event) => {
      if (!cancelled && event.sessionId.toLowerCase() === sessionId.toLowerCase()) setReloadKey((key) => key + 1);
    }).then((release) => {
      if (cancelled) release();
      else unsubscribe = release;
    }).catch(() => undefined);
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [sessionId]);
  React.useEffect(() => {
    if (!sessionId) return undefined;
    let cancelled = false;
    let timer: number | null = null;
    const load = async () => {
      try {
        const next = await listShellJobs(sessionId, showAll, showAll ? undefined : FINISHED_JOB_LIMIT);
        if (cancelled) return;
        setList(next);
        setError(null);
        if (visible && (next?.runningCount ?? 0) > 0) {
          timer = window.setTimeout(() => void load(), LIVE_REFRESH_MS);
        }
      } catch (loadError) {
        if (!cancelled) setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    };
    void load();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [reloadKey, sessionId, showAll, visible]);
  const reload = React.useCallback(() => setReloadKey((key) => key + 1), []);
  return { error, list, reload, setShowAll, showAll };
}

// ActivityFooter renders the clear and show-all controls of one page.
// ActivityFooter 渲染一个页面的清理与显示全部控件。
function ActivityFooter(props: {
  // Number of hidden ended entries.
  // 被隐藏的已结束条目数。
  hiddenCount: number;
  // Whether ended entries can be cleared now.
  // 当前是否有可清理的已结束条目。
  clearable: boolean;
  // Page state.
  // 页面状态。
  state: ActivityListState;
  // Clears the ended entries.
  // 清理已结束的条目。
  onClear: () => Promise<void>;
}) {
  const { t } = useI18n();
  const [clearing, setClearing] = React.useState(false);
  return (
    <div className="vk-act-foot">
      <Button
        disabled={!props.clearable || clearing}
        onClick={() => {
          setClearing(true);
          void props.onClear()
            .then(() => props.state.reload())
            .catch((error: unknown) => toast.error(error instanceof Error ? error.message : String(error)))
            .finally(() => setClearing(false));
        }}
        size="xs"
        type="button"
        variant="ghost"
      >
        <Eraser size={13} />
        {t("sessionActivity.clearEnded")}
      </Button>
      <Button
        aria-pressed={props.state.showAll}
        onClick={() => props.state.setShowAll(!props.state.showAll)}
        size="xs"
        type="button"
        variant="ghost"
      >
        {props.state.showAll
          ? t("sessionActivity.showRecent")
          : t("sessionActivity.showAll").replace("{count}", String(props.hiddenCount))}
      </Button>
    </div>
  );
}

// SubagentActivityPage lists the subagents: running first, each with its latest tool calls and output lines.
// SubagentActivityPage 列出子代理：运行中在前，每个附带最近的工具调用与输出行。
function SubagentActivityPage(props: {
  // Root session.
  // 根会话。
  sessionId: string;
  // Page data.
  // 页面数据。
  state: SubagentActivityState;
  // Opens one subagent transcript tab.
  // 打开一个子代理转录页签。
  onOpenSubagent: (parentConversationId: string, taskId: string) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const entries = props.state.snapshot?.subagents ?? [];
  const ended = entries.filter((entry) => !entry.task.isRunning && !entry.cleared).length;
  return (
    <>
      <div className="vk-act-scroll">
        {props.state.error ? <p className="vk-act-error">{props.state.error}</p> : null}
        {props.state.snapshot && entries.length === 0 ? <p className="vk-act-none">{t("sessionActivity.noSubagents")}</p> : null}
        <ul className="vk-act-list">
          {entries.map((entry) => (
            <SubagentActivityRow
              entry={entry}
              key={entry.task.taskId}
              onOpen={() => void props.onOpenSubagent(props.sessionId, entry.task.taskId)}
            />
          ))}
        </ul>
      </div>
      <ActivityFooter
        clearable={ended > 0}
        hiddenCount={props.state.snapshot?.clearedCount ?? 0}
        state={props.state}
        onClear={() => clearSessionActivity(props.sessionId, "subagents")}
      />
    </>
  );
}

// SubagentActivityRow renders one subagent; clicking opens its transcript.
// SubagentActivityRow 渲染一个子代理；点击打开其转录。
function SubagentActivityRow(props: {
  // Subagent row.
  // 子代理行。
  entry: SubagentActivity;
  // Opens the transcript.
  // 打开转录。
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const { task } = props.entry;
  return (
    <li className="vk-act-item" data-cleared={props.entry.cleared ? "" : undefined}>
      <button className="vk-act-agent" onClick={props.onOpen} type="button">
        <span className="vk-act-row">
          <Bot size={13} />
          <strong>{task.label}</strong>
          <small data-tone={conversationSessionSubagentRunStateTone(task.runState)}>
            {conversationSessionSubagentRunStateLabel(task.runState, t)}
          </small>
        </span>
        {task.description ? <span className="vk-act-desc">{task.description}</span> : null}
        {props.entry.recentTools.length > 0 ? (
          <span className="vk-act-tools">
            {props.entry.recentTools.map((tool, index) => (
              <span className="vk-act-tool" data-status={tool.status} key={`${tool.toolName}:${index}`}>
                <code>{tool.toolName}</code>
                {tool.summary ? <span>{tool.summary}</span> : null}
              </span>
            ))}
          </span>
        ) : null}
        {props.entry.recentOutput.length > 0 ? (
          <span className="vk-act-tail">{props.entry.recentOutput.join("\n")}</span>
        ) : null}
      </button>
    </li>
  );
}

// ShellJobPage lists the command jobs: running first with their latest lines, then the latest finished ones.
// ShellJobPage 列出命令任务：运行中在前并附最近输出行，其后是最近结束的任务。
function ShellJobPage(props: {
  // Root session.
  // 根会话。
  sessionId: string;
  // Page data.
  // 页面数据。
  state: ShellJobListState;
}) {
  const { t } = useI18n();
  const jobs = props.state.list?.jobs ?? [];
  const ended = jobs.filter((job) => !job.running && !job.cleared).length;
  const [expandedJobId, setExpandedJobId] = React.useState<string | null>(null);
  return (
    <>
      <div className="vk-act-scroll">
        {props.state.error ? <p className="vk-act-error">{props.state.error}</p> : null}
        {props.state.list && jobs.length === 0 ? <p className="vk-act-none">{t("sessionActivity.noCommands")}</p> : null}
        <ul className="vk-act-list">
          {jobs.map((job) => (
            <ShellJobRow
              expanded={expandedJobId === job.jobId}
              job={job}
              key={job.jobId}
              sessionId={props.sessionId}
              onExpandedChange={(expanded) => setExpandedJobId(expanded ? job.jobId : null)}
              onStopped={props.state.reload}
            />
          ))}
        </ul>
      </div>
      <ActivityFooter
        clearable={ended > 0}
        hiddenCount={Math.max(0, (props.state.list?.finishedCount ?? 0) - jobs.filter((job) => !job.running).length)}
        state={props.state}
        onClear={() => clearSessionActivity(props.sessionId, "commands")}
      />
    </>
  );
}

// ShellJobRow renders one job with its purpose, command, state and, when expanded, its scrollable log.
// ShellJobRow 渲染一个任务的用途、命令与状态，展开后显示可滚动日志。
function ShellJobRow(props: {
  // Root session.
  // 根会话。
  sessionId: string;
  // Job row.
  // 任务行。
  job: ShellJobView;
  // Whether the log is expanded.
  // 日志是否展开。
  expanded: boolean;
  // Expands or collapses the log.
  // 展开或收起日志。
  onExpandedChange: (expanded: boolean) => void;
  // Called after a stop request.
  // 停止请求之后调用。
  onStopped: () => void;
}) {
  const { t } = useI18n();
  const { job } = props;
  const [stopping, setStopping] = React.useState(false);
  const label = job.purpose.trim() || job.command;
  const meta = [
    shellJobStatusLabel(job.status, t),
    job.exitCode !== undefined && !job.running ? t("sessionActivity.exitCode").replace("{code}", String(job.exitCode)) : null,
    formatActivityDuration(job.durationMs),
    job.background ? t("sessionActivity.background") : null,
    t("sessionActivity.lines").replace("{count}", String(job.totalLines)),
  ].filter((part): part is string => Boolean(part));
  return (
    <li className="vk-act-item" data-cleared={job.cleared ? "" : undefined} data-state={job.status}>
      <div className="vk-act-row">
        <button
          aria-expanded={props.expanded}
          aria-label={props.expanded ? t("sessionActivity.collapseLog") : t("sessionActivity.expandLog")}
          className="vk-act-toggle"
          disabled={!job.logAvailable}
          onClick={() => props.onExpandedChange(!props.expanded)}
          type="button"
        >
          {props.expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          <span className="vk-act-dot" data-state={job.status} />
          <strong title={label}>{label}</strong>
        </button>
        {job.running ? (
          <IconButton
            aria-label={t("sessionActivity.stop")}
            disabled={stopping}
            onClick={() => {
              setStopping(true);
              void stopShellJob(props.sessionId, job.jobId)
                .then(() => props.onStopped())
                .catch((error: unknown) => toast.error(error instanceof Error ? error.message : String(error)))
                .finally(() => setStopping(false));
            }}
            size="xs"
            title={t("sessionActivity.stop")}
          >
            <Square size={12} />
          </IconButton>
        ) : null}
      </div>
      <code className="vk-act-cmd" title={job.command}>{job.command}</code>
      <span className="vk-act-meta">
        <span>{job.jobId}</span>
        {meta.map((part) => <span key={part}>{part}</span>)}
      </span>
      {job.failureReason ? <span className="vk-act-reason">{job.failureReason}</span> : null}
      {props.expanded ? (
        <ShellJobLogView jobId={job.jobId} running={job.running} sessionId={props.sessionId} />
      ) : job.running && job.recentLines.length > 0 ? (
        <span className="vk-act-tail">{job.recentLines.join("\n")}</span>
      ) : null}
    </li>
  );
}

// shellJobStatusLabel maps a job status word to its label.
// shellJobStatusLabel 将任务状态词映射为其标签。
export function shellJobStatusLabel(status: string, t: (key: TranslationKey) => string): string {
  switch (status) {
    case "running":
      return t("sessionActivity.status.running");
    case "completed":
      return t("sessionActivity.status.completed");
    case "failed":
      return t("sessionActivity.status.failed");
    case "timed_out":
      return t("sessionActivity.status.timedOut");
    case "stopped":
      return t("sessionActivity.status.stopped");
    default:
      return t("sessionActivity.status.lost");
  }
}
