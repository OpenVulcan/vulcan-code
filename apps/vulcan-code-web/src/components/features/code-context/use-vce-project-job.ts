import { translateActive } from "@/i18n";
import * as React from "react";
import type { VceJobSummary } from "@/types";
import { useVceJobObserverStore } from "./vce-job-observer-context";
import { EMPTY_VCE_OBSERVED_JOB, isVceJobActive } from "./vce-job-observer-store";

export { isVceJobActive, mergeVceJobReceipt } from "./vce-job-observer-store";

// Follow projectId using the workbench's shared observation rather than component-local IO.
// 使用工作台共享观察跟随 projectId，而非组件局部输入输出。
// Returns the receipt, execution state, explicit error, and exact receipt/cancellation actions.
// 返回回执、执行状态、明确错误及精确回执／取消操作。
export function useVceProjectJob(projectId: string | null) {
  // store changes only when the workbench service lifetime changes.
  // store 仅在工作台服务生命周期变化时改变。
  const store = useVceJobObserverStore();
  // scope protects delayed actions across project changes, including A-to-B-to-A navigation.
  // scope 保护跨项目变化的迟到操作，包含甲到乙再到甲的导航。
  const scope = React.useMemo(() => ({ projectId, store }), [projectId, store]);
  // mounted admits UI receipts only for the currently committed component lifetime.
  // mounted 仅为当前已提交组件生命周期接收界面回执。
  const mounted = React.useRef<object | null>(null);
  React.useEffect(() => {
    mounted.current = scope;
    return () => { if (mounted.current === scope) mounted.current = null; };
  }, [scope]);
  // subscribe/getSnapshot remain stable across unrelated component renders.
  // subscribe/getSnapshot 在无关组件渲染期间保持稳定。
  const subscribe = React.useCallback((listener: () => void) => store.subscribe(projectId, listener), [projectId, store]);
  const getSnapshot = React.useCallback(() => store.snapshot(projectId), [projectId, store]);
  // snapshot is shared by reference among every consumer of this exact project.
  // snapshot 在此精确项目的全部消费者之间共享引用。
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot, () => EMPTY_VCE_OBSERVED_JOB);
  // receive validates receipt ownership and ignores actions from a retired UI scope; returns nothing.
  // receive 验证回执归属并忽略已退役界面作用域的操作；无返回值。
  const receive = React.useCallback((receipt: VceJobSummary): void => {
    if (projectId === null || receipt.projectId !== projectId) throw new Error(translateActive("codeContext.observer.receiptMismatch"));
    if (mounted.current === scope) store.receive(projectId, receipt);
  }, [projectId, store, scope]);
  // cancel returns the shared exact-job command; retired views cannot initiate a later cancellation.
  // cancel 返回共享精确作业命令；已退役视图不能发起后续取消。
  const cancel = React.useCallback(async (): Promise<void> => {
    if (projectId !== null && snapshot.job !== null && mounted.current === scope) await store.cancel(projectId, snapshot.job.jobId);
  }, [projectId, store, scope, snapshot.job]);
  return { job: snapshot.job, active: isVceJobActive(snapshot.job), error: snapshot.error, receive, cancel, cancelPending: snapshot.cancelPending };
}
