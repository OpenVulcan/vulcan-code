import * as React from "react";
import { executePluginRuntimeCommand, readPluginRuntime, type PluginRuntimeCommand, type PluginRuntimeObservation } from "@/service/plugin-runtime-service";

// usePluginRuntimeManagement owns view observations, never the underlying durable transaction.
// usePluginRuntimeManagement 持有视图观测，绝不持有底层持久事务。
// Returns explicit refresh/execute actions and unresolved original intent for review or retry.
// 返回明确刷新／执行操作及待核查或重试的原意图。
export function usePluginRuntimeManagement() {
  // Observation, freshness and diagnostics remain separate so a failed refresh cannot authorize writes.
  // 观测、新鲜度及诊断分离，防止失败刷新授权写入。
  const [observation, setObservation] = React.useState<PluginRuntimeObservation | null>(null);
  const [fresh, setFresh] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  const [pending, setPending] = React.useState<PluginRuntimeCommand | null>(null);
  // epoch prevents an old read from publishing after remount; mutationBusy closes the same-tick double-click window.
  // epoch 防止旧读取在重新挂载后发布；mutationBusy 关闭同一时刻的重复点击窗口。
  const epoch = React.useRef(0);
  const mounted = React.useRef(false);
  const mutationBusy = React.useRef(false);
  const pendingRef = React.useRef<PluginRuntimeCommand | null>(null);
  // freshRef revokes write eligibility synchronously, before React flushes disabled controls.
  // freshRef 在 React 刷新禁用控件之前同步撤销写入资格。
  const freshRef = React.useRef(false);

  // refresh samples the service without changing a draft or advancing a retained transaction; returns nothing.
  // refresh 采样服务，不改变草稿或推进保留事务；无返回值。
  const refresh = React.useCallback(async (): Promise<void> => {
    if (mutationBusy.current) return;
    // requested fences previous reads, including StrictMode's initial discarded effect.
    // requested 隔离先前读取，包括 StrictMode 初次丢弃的副作用。
    const requested = ++epoch.current;
    freshRef.current = false; setBusy(true); setFresh(false);
    try {
      // next is published only by the latest mounted observer.
      // next 仅由最新已挂载观测者发布。
      const next = await readPluginRuntime();
      if (mounted.current && requested === epoch.current) {
        freshRef.current = true; setObservation(next); setFresh(true); setError("");
      }
    } catch (reason) {
      if (mounted.current && requested === epoch.current) setError(String(reason));
    } finally {
      if (mounted.current && requested === epoch.current) setBusy(false);
    }
  }, []);

  React.useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => { mounted.current = false; freshRef.current = false; epoch.current += 1; };
  }, [refresh]);

  // execute sends the original command; an uncertain result retains it and never invents a replacement request.
  // execute 发送原 command；结果不确定时保留原请求，绝不生成替代请求。
  const execute = async (command: PluginRuntimeCommand): Promise<void> => {
    if (mutationBusy.current || (pendingRef.current && command !== pendingRef.current)
      || (!pendingRef.current && !freshRef.current)) return;
    mutationBusy.current = true; epoch.current += 1;
    freshRef.current = false; pendingRef.current = command; setPending(command); setBusy(true); setFresh(false); setError("");
    try {
      await executePluginRuntimeCommand(command);
      pendingRef.current = null;
      if (mounted.current) setPending(null);
    } catch (reason) {
      if (mounted.current) setError(String(reason));
    } finally {
      mutationBusy.current = false;
      if (mounted.current) setBusy(false);
    }
    // Read failure after an acknowledged write remains a stale observation, not a reason to repeat the write.
    // 已确认写入后的读取失败仍为陈旧观测，不构成重复写入的理由。
    if (mounted.current && !pendingRef.current) await refresh();
  };

  // discardTracking acknowledges only this view's uncertainty; it never cancels host work.
  // discardTracking 仅确认本视图的不确定状态；绝不取消宿主工作。
  const discardTracking = (): void => {
    if (mutationBusy.current || !freshRef.current) return;
    pendingRef.current = null; setPending(null);
  };

  return { observation, fresh, busy, error, pending, refresh, execute, discardTracking };
}
