// Running background command jobs of the selected session, kept current from job notices; no polling.
// 当前选中会话中运行中的后台命令任务数，由任务通知保持最新；不轮询。
import * as React from "react";
import { useTerminalDisplayConnectionState } from "@/components/terminal/use-terminal-display-connection-epoch";
import { listShellJobs } from "@/service/session-activity-service";
import { SHELL_JOB_CHANGED_TOPIC } from "@/service/shell-job-events";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { ShellJobChangedEvent } from "@/types";

/** Returns the running background jobs of `sessionId`; 0 without a session. The count is read once per session and
 * connection and then follows the job notices.
 * 返回 `sessionId` 中运行中的后台任务数；没有会话时为 0。每个会话与连接读取一次，之后跟随任务通知。 */
export function useSessionBackgroundJobCount(sessionId: string | null): number {
  const { connectionAvailable, connectionEpoch } = useTerminalDisplayConnectionState();
  const [count, setCount] = React.useState<{ sessionId: string; value: number } | null>(null);
  React.useEffect(() => {
    if (!sessionId || !connectionAvailable || !hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    // A notice that arrives before the first reading wins over it, since it is newer.
    // 在首次读取之前到达的通知比读取更新，因此以通知为准。
    let noticed = false;
    void serviceClient.subscribe<ShellJobChangedEvent>(SHELL_JOB_CHANGED_TOPIC, (event) => {
      if (cancelled || event.sessionId.toLowerCase() !== sessionId.toLowerCase()) return;
      noticed = true;
      setCount({ sessionId, value: event.backgroundRunningCount });
    }).then((release) => {
      if (cancelled) {
        release();
        return;
      }
      unsubscribe = release;
      return listShellJobs(sessionId, false, 0).then((list) => {
        if (!cancelled && !noticed && list) setCount({ sessionId, value: list.backgroundRunningCount });
      });
    }).catch((error: unknown) => console.warn("Failed to read the background command count.", error));
    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, [connectionAvailable, connectionEpoch, sessionId]);
  return count && count.sessionId === sessionId ? count.value : 0;
}
