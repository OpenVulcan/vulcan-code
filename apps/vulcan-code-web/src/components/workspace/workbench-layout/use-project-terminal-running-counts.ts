import React from "react";
import { listProjectTerminals } from "@/service/terminal-service";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import type { ProjectTerminalRunningCountChangedEvent } from "@/types";

// ProjectTerminalRunningCount is the newest accepted running-terminal count for one project.
// ProjectTerminalRunningCount 是单个项目最新被接受的运行中终端数量。
export interface ProjectTerminalRunningCount {
  // Number of terminals whose lifecycle status is running.
  // 生命周期状态为运行中的终端数量。
  runningCount: number;
  // Monotonic revision used to reject stale metadata queries and events.
  // 用于拒绝过期元数据查询与事件的单调修订号。
  revision: number;
}

// useProjectTerminalRunningCounts tracks running-terminal counts per project without attaching terminal content.
// useProjectTerminalRunningCounts 按项目跟踪运行中终端数量，不挂接终端内容。
// Parameters: serviceInstanceEpoch resets the subscription after a reconnect; activeProjectId receives one bootstrap query.
// 参数：serviceInstanceEpoch 在重连后重置订阅；activeProjectId 获得一次初始查询。
// Returns: the newest accepted count keyed by project id.
// 返回值：按项目标识索引的最新已接受数量。
export function useProjectTerminalRunningCounts(
  serviceInstanceEpoch: number,
  activeProjectId: string | null,
): Record<string, ProjectTerminalRunningCount> {
  // projectTerminalRunningCounts stores count-only lifecycle state independently from terminal content.
  // projectTerminalRunningCounts 独立于终端内容保存仅包含数量的生命周期状态。
  const [projectTerminalRunningCounts, setProjectTerminalRunningCounts] = React.useState<Record<
    string,
    ProjectTerminalRunningCount
  >>({});
  // projectTerminalCountListenerReady closes the bootstrap race between event registration and the first query.
  // projectTerminalCountListenerReady 消除事件注册与首次查询之间的初始竞态。
  const [projectTerminalCountListenerReady, setProjectTerminalCountListenerReady] = React.useState(false);
  React.useEffect(() => {
    setProjectTerminalCountListenerReady(false);
    setProjectTerminalRunningCounts({});
    if (!hasServiceRuntime()) return undefined;
    let cancelled = false;
    let unsubscribe: (() => void) | null = null;
    // subscription receives project metadata only; terminal output remains bound to visible display attachments.
    // subscription 仅接收项目元数据；终端输出仍严格绑定可见显示挂接。
    const subscription = serviceClient.subscribe<ProjectTerminalRunningCountChangedEvent>(
      "project.terminal.running-count.changed",
      (event) => {
        if (cancelled) return;
        setProjectTerminalRunningCounts((current) => {
          const previous = current[event.projectId];
          if (previous && previous.revision > event.revision) return current;
          return {
            ...current,
            [event.projectId]: {
              runningCount: event.runningCount,
              revision: event.revision,
            },
          };
        });
      },
    );
    void subscription.then((nextUnsubscribe) => {
      if (cancelled) nextUnsubscribe();
      else {
        unsubscribe = nextUnsubscribe;
        setProjectTerminalCountListenerReady(true);
      }
    }).catch(() => undefined);
    return () => {
      cancelled = true;
      unsubscribe?.();
      unsubscribe = null;
    };
  }, [serviceInstanceEpoch]);
  React.useEffect(() => {
    if (!activeProjectId || !projectTerminalCountListenerReady || !hasServiceRuntime()) return undefined;
    let cancelled = false;
    // loadRunningCount performs one metadata bootstrap for the active project without attaching screen content.
    // loadRunningCount 为当前活动项目执行一次元数据初始加载，不挂接屏幕内容。
    const loadRunningCount = async (): Promise<void> => {
      const catalog = await listProjectTerminals(activeProjectId);
      if (cancelled || !catalog || catalog.projectId !== activeProjectId) return;
      const runningCount = catalog.terminals.filter((terminal) => terminal.status === "running").length;
      setProjectTerminalRunningCounts((current) => {
        const previous = current[activeProjectId];
        if (previous && previous.revision > catalog.runningCountRevision) return current;
        return {
          ...current,
          [activeProjectId]: {
            runningCount,
            revision: catalog.runningCountRevision,
          },
        };
      });
    };
    void loadRunningCount().catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, projectTerminalCountListenerReady, serviceInstanceEpoch]);
  return projectTerminalRunningCounts;
}
