// Session activity panel types: background shell jobs and subagents of one root session. Mirrors
// `vulcan-service-api` `session_activity.rs`; every text field is already redacted by Core.
// 会话活动面板类型：一个根会话的后台命令任务与子代理。镜像 `vulcan-service-api` 的 `session_activity.rs`；
// 所有文本字段均已由 Core 脱敏。
import type { SubagentTaskSummary } from "./planning-questions";

// ShellJobView is one command job of a session, running or finished.
// ShellJobView 是会话中的一个命令任务，运行中或已结束。
export interface ShellJobView {
  // Stable job id shown to the AI.
  // 显示给 AI 的稳定任务标识。
  jobId: string;
  // Command text.
  // 命令文本。
  command: string;
  // Purpose the AI gave for the command.
  // AI 为该命令给出的用途说明。
  purpose: string;
  // Shell family that runs the command.
  // 运行该命令的 Shell 族。
  shell: string;
  // Working directory.
  // 工作目录。
  cwd: string;
  // Start time in Unix milliseconds.
  // 开始时间（Unix 毫秒）。
  startedAtMs: number;
  // API call that started the command, when it came from a tool call.
  // 启动该命令的 API 调用（来自工具调用时）。
  apiCallId?: number;
  // Tool call that started the command.
  // 启动该命令的工具调用。
  toolCallId?: number;
  // Whether the job runs, or ran, in the background.
  // 任务是否在后台运行（或曾在后台运行）。
  background: boolean;
  // Whether the process still runs.
  // 进程是否仍在运行。
  running: boolean;
  // Status word: running, completed, failed, timed_out, stopped or lost.
  // 状态词：running、completed、failed、timed_out、stopped 或 lost。
  status: string;
  // Exit code once finished, when the process reported one.
  // 结束后的退出码（进程报告时）。
  exitCode?: number;
  // Elapsed or total duration in milliseconds.
  // 已运行或总耗时（毫秒）。
  durationMs: number;
  // Finish time in Unix milliseconds.
  // 结束时间（Unix 毫秒）。
  finishedAtMs?: number;
  // Why the job failed or was stopped.
  // 任务失败或被停止的原因。
  failureReason?: string;
  // Number of output lines written so far.
  // 目前已写出的输出行数。
  totalLines: number;
  // Number of output bytes written so far.
  // 目前已写出的输出字节数。
  totalBytes: number;
  // Whether the output log can still be read.
  // 输出日志是否仍可读取。
  logAvailable: boolean;
  // Time of the latest output in Unix milliseconds.
  // 最近一次输出的时间（Unix 毫秒）。
  lastOutputAtMs?: number;
  // Latest output lines of a running job, oldest first.
  // 运行中任务的最近输出行，从旧到新。
  recentLines: string[];
  // Whether the job finished before the latest clear.
  // 任务是否在最近一次清理之前已结束。
  cleared: boolean;
}

// ShellJobList is the command list of one session.
// ShellJobList 是一个会话的命令列表。
export interface ShellJobList {
  // Session the list belongs to.
  // 列表所属会话。
  sessionId: string;
  // Jobs, running first, then newest first.
  // 任务列表：运行中在前，其余按最新在前。
  jobs: ShellJobView[];
  // Running jobs, foreground and background.
  // 运行中的任务（前台与后台）。
  runningCount: number;
  // Running background jobs.
  // 运行中的后台任务。
  backgroundRunningCount: number;
  // Finished jobs, cleared ones included.
  // 已结束任务数，含已清理的任务。
  finishedCount: number;
  // Finished jobs hidden by the latest clear.
  // 被最近一次清理隐藏的已结束任务数。
  clearedCount: number;
  // Number of recent lines carried by running jobs.
  // 运行中任务携带的最近行数。
  recentLineLimit: number;
}

// ShellJobOutputRow is one display row of a job log; long lines are split into continuation rows.
// ShellJobOutputRow 是任务日志的一行显示；过长的行会拆成续行。
export interface ShellJobOutputRow {
  // Zero-based log line number.
  // 从零开始的日志行号。
  line: number;
  // "text" for output, "gap" for lines that were skipped to stay within the byte budget.
  // "text" 表示输出，"gap" 表示为保持字节预算而跳过的行。
  kind: "text" | "gap";
  // Whether this row continues the previous row of the same line.
  // 该行是否接续同一日志行的上一显示行。
  continuation: boolean;
  // Row text.
  // 行文本。
  text: string;
}

// ShellJobOutputPage is one page of a job log.
// ShellJobOutputPage 是任务日志的一页。
export interface ShellJobOutputPage {
  // Job id.
  // 任务标识。
  jobId: string;
  // Rows, oldest first.
  // 显示行，从旧到新。
  rows: ShellJobOutputRow[];
  // Whether earlier lines exist before the first row.
  // 第一行之前是否还有更早的行。
  hasEarlier: boolean;
  // First line after this page, when the page stopped early.
  // 本页提前结束时，下一页的起始行。
  nextLine?: number;
  // Total lines written so far.
  // 目前已写出的总行数。
  totalLines: number;
  // Whether the job still runs.
  // 任务是否仍在运行。
  running: boolean;
}

// ShellJobChangedEvent announces that a job of one session started, moved to the background or finished.
// ShellJobChangedEvent 通知某会话的任务已开始、已转入后台或已结束。
export interface ShellJobChangedEvent {
  // Owner session.
  // 所属会话。
  sessionId: string;
  // Job id.
  // 任务标识。
  jobId: string;
  // What changed.
  // 变化类型。
  change: "started" | "backgrounded" | "finished";
  // Running jobs of the session after the change.
  // 变化后该会话运行中的任务数。
  runningCount: number;
  // Running background jobs of the session after the change.
  // 变化后该会话运行中的后台任务数。
  backgroundRunningCount: number;
}

// SubagentActivityTool is one recent tool call of a subagent.
// SubagentActivityTool 是子代理最近的一次工具调用。
export interface SubagentActivityTool {
  // Tool name.
  // 工具名。
  toolName: string;
  // Short argument summary.
  // 简短参数摘要。
  summary?: string;
  // Call status.
  // 调用状态。
  status: string;
}

// SubagentActivity is one subagent row of the activity panel.
// SubagentActivity 是活动面板中的一个子代理行。
export interface SubagentActivity {
  // Navigation summary of the subagent.
  // 子代理的导航摘要。
  task: SubagentTaskSummary;
  // Creation time in Unix milliseconds.
  // 创建时间（Unix 毫秒）。
  createdAtMs: number;
  // Latest update in Unix milliseconds.
  // 最近更新时间（Unix 毫秒）。
  updatedAtMs: number;
  // Whether it ended before the latest clear.
  // 是否在最近一次清理之前已结束。
  cleared: boolean;
  // Latest tool calls, oldest first.
  // 最近的工具调用，从旧到新。
  recentTools: SubagentActivityTool[];
  // Latest output lines, oldest first.
  // 最近的输出行，从旧到新。
  recentOutput: string[];
}

// SessionActivitySnapshot is the subagent list of one root session.
// SessionActivitySnapshot 是一个根会话的子代理列表。
export interface SessionActivitySnapshot {
  // Root session.
  // 根会话。
  sessionId: string;
  // Subagents, running first, then latest first.
  // 子代理：运行中在前，其余按最近更新在前。
  subagents: SubagentActivity[];
  // Ended subagents hidden by the latest clear.
  // 被最近一次清理隐藏的已结束子代理数。
  clearedCount: number;
}

// SessionActivityScope names the list a clear applies to.
// SessionActivityScope 指明清理作用的列表。
export type SessionActivityScope = "subagents" | "commands";
