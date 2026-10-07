import * as React from "react";
import type { PluginEnvironmentInstallTaskStatus } from "../../../service/skills-actions-service";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// PluginEnvironmentTasks is the Core-observed environment readiness and install-task view of the plugin page.
// PluginEnvironmentTasks 是插件页面经 Core 观测的环境就绪状态与安装任务视图。
export type PluginEnvironmentTasks = {
  // environmentReadyByPlugin records the service-checked readiness of each exact plugin generation.
  // environmentReadyByPlugin 记录每个精确插件代次经服务检查后的就绪状态。
  environmentReadyByPlugin: Record<string, boolean>;
  // installTasksByPlugin reflects Core-owned work, including work started by another client.
  // installTasksByPlugin 反映 Core 持有的任务，包括其他客户端启动的任务。
  installTasksByPlugin: Record<string, PluginEnvironmentInstallTaskStatus>;
  // recordInstallTask publishes a task Core just accepted so polling starts without waiting for a refresh.
  // recordInstallTask 发布 Core 刚接受的任务，使轮询无需等待刷新即开始。
  recordInstallTask: (pluginId: string, task: PluginEnvironmentInstallTaskStatus) => void;
};

// usePluginEnvironmentTasks reads readiness for the committed catalog and follows unfinished Core install tasks.
// usePluginEnvironmentTasks 读取已提交目录的就绪状态，并跟踪尚未完成的 Core 安装任务。
//
// Parameters: actions are the service-backed plugin actions, plugins the Vulcan Plugin rows, and onError
// receives polling diagnostics. Unmounting stops observation only; it never cancels an installation.
// 参数：actions 是服务支持的插件动作，plugins 是 Vulcan 插件行，onError 接收轮询诊断。
// 卸载只停止观测，绝不取消安装。
// Returns: the current readiness and task maps plus the task recorder.
// 返回值：当前就绪状态、任务映射及任务记录函数。
export function usePluginEnvironmentTasks(
  actions: SkillActionProps,
  plugins: PluginSummary[],
  onError: (message: string) => void,
): PluginEnvironmentTasks {
  const [environmentReadyByPlugin, setEnvironmentReadyByPlugin] = React.useState<Record<string, boolean>>({});
  const [installTasksByPlugin, setInstallTasksByPlugin] = React.useState<Record<string, PluginEnvironmentInstallTaskStatus>>({});
  // Refresh readiness when the committed plugin catalog changes; stale responses cannot replace a newer view.
  // 已提交插件目录变化时刷新就绪状态；过期响应不能覆盖较新的视图。
  React.useEffect(() => {
    // active prevents an outdated catalog request from publishing readiness after cleanup.
    // active 防止过期目录请求在清理后发布就绪状态。
    let active = true;
    void Promise.all(plugins.map(async (plugin) => {
      // ready is read from Core, which owns the plugin generation and private environment.
      // ready 从持有插件代次和私有环境的 Core 读取。
      const ready = await actions.onPluginEnvironmentReady(plugin.id).catch(() => false);
      const task = await actions.onPluginEnvironmentInstallStatusForPlugin(plugin.id).catch(() => null);
      return [plugin.id, ready, task] as const;
    })).then((entries) => {
      if (active) {
        setEnvironmentReadyByPlugin(Object.fromEntries(entries.map(([id, ready]) => [id, ready])));
        setInstallTasksByPlugin(Object.fromEntries(entries.flatMap(([id, , task]) => task ? [[id, task]] : [])));
      }
    });
    return () => { active = false; };
  }, [actions, plugins]);
  // Poll only unfinished Core tasks; unmounting the panel never cancels their installation.
  // 只轮询尚未完成的 Core 任务；卸载面板绝不取消安装。
  React.useEffect(() => {
    const pending = Object.values(installTasksByPlugin).filter((task) => task.state === "queued" || task.state === "running");
    if (pending.length === 0) return;
    let active = true;
    let polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const updates = await Promise.all(pending.map((task) => actions.onPluginEnvironmentInstallStatus(task.taskId)));
        if (!active) return;
        setInstallTasksByPlugin((current) => {
          const next = { ...current };
          for (const task of updates) {
            if (next[task.pluginId]?.taskId === task.taskId) next[task.pluginId] = task;
          }
          return next;
        });
        for (const task of updates) {
          if (task.state === "succeeded") {
            const ready = await actions.onPluginEnvironmentReady(task.pluginId);
            if (active) setEnvironmentReadyByPlugin((current) => ({ ...current, [task.pluginId]: ready }));
          }
        }
      } catch (reason) {
        if (active) onError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => { void poll(); }, 1000);
    return () => { active = false; window.clearInterval(timer); };
  }, [installTasksByPlugin, actions, onError]);
  // A second client can start installation while this panel is open, so refresh Core's latest task by plugin.
  // 其他客户端可能在本面板打开期间启动安装，因此按插件刷新 Core 的最近任务。
  React.useEffect(() => {
    if (plugins.length === 0) return;
    let active = true;
    const synchronize = async () => {
      const entries = await Promise.all(plugins.map(async (plugin) => [
        plugin.id,
        await actions.onPluginEnvironmentInstallStatusForPlugin(plugin.id).catch(() => null),
      ] as const));
      if (!active) return;
      setInstallTasksByPlugin(Object.fromEntries(entries.flatMap(([id, task]) => task ? [[id, task] as const] : [])));
      const succeeded = entries.filter(([, task]) => task?.state === "succeeded");
      const readiness = await Promise.all(succeeded.map(async ([id]) => [
        id,
        await actions.onPluginEnvironmentReady(id).catch(() => false),
      ] as const));
      if (active) setEnvironmentReadyByPlugin((current) => ({ ...current, ...Object.fromEntries(readiness) }));
    };
    const timer = window.setInterval(() => { void synchronize(); }, 3000);
    return () => { active = false; window.clearInterval(timer); };
  }, [actions, plugins]);
  const recordInstallTask = React.useCallback((pluginId: string, task: PluginEnvironmentInstallTaskStatus) => {
    setInstallTasksByPlugin((current) => ({ ...current, [pluginId]: task }));
  }, []);
  return { environmentReadyByPlugin, installTasksByPlugin, recordInstallTask };
}
