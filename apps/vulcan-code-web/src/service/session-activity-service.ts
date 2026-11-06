// Session activity service calls shared by the eager workbench: listing jobs, backgrounding a command and the
// running job count. Calls used only by the lazily loaded activity panel live in session-activity-panel-service.ts.
// 工作台首屏共享的会话活动服务调用：列出任务、将命令转入后台与运行中任务数。只由按需加载的活动面板使用的调用
// 位于 session-activity-panel-service.ts。
import type { ShellJobList } from "@/types";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";

// listShellJobs lists the command jobs of one session; finishedLimit caps the finished ones.
// listShellJobs 列出一个会话的命令任务；finishedLimit 限制已结束任务的数量。
export async function listShellJobs(
  sessionId: string,
  includeCleared: boolean,
  finishedLimit?: number,
): Promise<ShellJobList | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ShellJobList>("service_list_shell_jobs", {
    finishedLimit: finishedLimit ?? null,
    includeCleared,
    sessionId,
  });
}

// backgroundShellCommand moves a running shell_command call to the background; false when it is no longer running.
// backgroundShellCommand 将运行中的 shell_command 调用转入后台；调用已不在运行时返回 false。
export async function backgroundShellCommand(sessionId: string, apiCallId: number, toolCallId: number): Promise<boolean> {
  if (!hasServiceRuntime()) return false;
  return serviceClient.request<boolean>("service_background_shell_command", { apiCallId, sessionId, toolCallId });
}

// countRunningShellJobs counts the running background jobs of every session; 0 without a service.
// countRunningShellJobs 统计所有会话中运行中的后台任务；没有服务时为 0。
export async function countRunningShellJobs(): Promise<number> {
  if (!hasServiceRuntime()) return 0;
  return serviceClient.request<number>("service_running_shell_job_count", {});
}
