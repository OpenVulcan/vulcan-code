// Session activity service calls used only by the lazily loaded activity panel: job logs, stopping a job, the
// subagent list and the clear marker.
// 只由按需加载的活动面板使用的会话活动服务调用：任务日志、停止任务、子代理列表与清除标记。
import type {
  SessionActivityScope,
  SessionActivitySnapshot,
  ShellJobOutputPage,
} from "@/types";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";

// ShellJobOutputQuery selects a page: the tail by default, rows ending before `beforeLine`, or rows from `fromLine`.
// ShellJobOutputQuery 选择一页：默认为末尾，或 `beforeLine` 之前的行，或从 `fromLine` 开始的行。
export interface ShellJobOutputQuery {
  // Read rows that end before this line.
  // 读取结束于该行之前的行。
  beforeLine?: number;
  // Read rows starting at this line.
  // 读取从该行开始的行。
  fromLine?: number;
  // Maximum log lines of the page.
  // 本页最多的日志行数。
  maxLines?: number;
}

// readShellJobOutput reads one bounded page of a job log.
// readShellJobOutput 读取任务日志中有界的一页。
export async function readShellJobOutput(
  sessionId: string,
  jobId: string,
  query: ShellJobOutputQuery = {},
): Promise<ShellJobOutputPage | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ShellJobOutputPage>("service_read_shell_job_output", {
    beforeLine: query.beforeLine ?? null,
    fromLine: query.fromLine ?? null,
    jobId,
    maxLines: query.maxLines ?? null,
    sessionId,
  });
}

// stopShellJob stops one running job; false when it already finished.
// stopShellJob 停止一个运行中的任务；任务已结束时返回 false。
export async function stopShellJob(sessionId: string, jobId: string): Promise<boolean> {
  if (!hasServiceRuntime()) return false;
  return serviceClient.request<boolean>("service_stop_shell_job", { jobId, sessionId });
}

// loadSessionActivity reads the subagent list of one root session.
// loadSessionActivity 读取一个根会话的子代理列表。
export async function loadSessionActivity(sessionId: string, includeCleared: boolean): Promise<SessionActivitySnapshot | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<SessionActivitySnapshot>("service_session_activity", { includeCleared, sessionId });
}

// clearSessionActivity hides the ended entries of one list; nothing is deleted.
// clearSessionActivity 隐藏一个列表中已结束的条目；不删除任何内容。
export async function clearSessionActivity(sessionId: string, scope: SessionActivityScope): Promise<void> {
  if (!hasServiceRuntime()) return;
  await serviceClient.request<unknown>("service_clear_session_activity", { scope, sessionId });
}
