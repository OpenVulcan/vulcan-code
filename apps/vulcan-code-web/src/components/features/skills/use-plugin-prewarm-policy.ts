import * as React from "react";
import { samePluginPrewarmPolicyRequest } from "@/service/plugin-prewarm-policy-contract";
import { executePluginPrewarmPolicyCommand, readPluginPrewarmPolicyStatus, type PluginPrewarmPolicyCommand } from "@/service/plugin-prewarm-policy-service";
import type { PluginPrewarmPolicyReceipt, PluginPrewarmPolicyStatus } from "@/types/plugin-prewarm-policy";

// PolicyObservation separates a complete read from acknowledged writes and unresolved original intent.
// PolicyObservation 将完整读取与已确认写入及未决原意图区分。
interface PolicyObservation {
  // status is the last complete atomic host projection; failed reads preserve it as stale evidence.
  // status 为最后完整原子宿主投影；读取失败时保留为陈旧证据。
  status: PluginPrewarmPolicyStatus | null;
  // fresh grants eligibility only after a successful complete read.
  // fresh 仅在完整读取成功后授予操作资格。
  fresh: boolean;
  // pending preserves the exact command after an unknown transport outcome.
  // pending 在传输结果未知后保留精确原操作。
  pending: PluginPrewarmPolicyCommand | null;
  // acknowledged preserves a receipt even if the following read fails.
  // acknowledged 在后续读取失败时仍保留已确认回执。
  acknowledged: PluginPrewarmPolicyReceipt | null;
  // error is local observation or transport failure, separate from the host receipt's diagnostics.
  // error 为本地观测或传输失败，与宿主回执诊断分离。
  error: string;
}

// policyReceiptAllows returns whether the host checkpoint permits an explicit management action.
// policyReceiptAllows 返回宿主检查点是否允许明确管理操作。
export function policyReceiptAllows(receipt: PluginPrewarmPolicyReceipt, kind: Exclude<PluginPrewarmPolicyCommand["kind"], "begin">): boolean {
  if (kind === "resume") return true;
  if (kind === "cancel") return receipt.phase === "prepared";
  return ["committed", "cancelled", "interrupted"].includes(receipt.phase) && receipt.persistenceError === undefined;
}

// usePluginPrewarmPolicy returns explicit policy controls; mounting never reads, writes, or initializes Lua.
// usePluginPrewarmPolicy 返回明确策略控件；挂载绝不读取、写入或初始化 Lua。
export function usePluginPrewarmPolicy() {
  // observation and retained share one snapshot; the ref fences same-tick handlers before React renders.
  // observation 与 retained 共用快照；引用在 React 渲染前隔离同一时刻处理器。
  const [observation, setObservation] = React.useState<PolicyObservation>({ status: null, fresh: false, pending: null, acknowledged: null, error: "" });
  const retained = React.useRef(observation);
  const [busy, setBusy] = React.useState(false);
  const busyRef = React.useRef(false);
  // epoch and mounted reject responses owned by an unmounted or StrictMode-discarded observer.
  // epoch 与 mounted 拒绝已卸载或 StrictMode 丢弃观测者的响应。
  const epoch = React.useRef(0);
  const mounted = React.useRef(false);
  React.useEffect(() => {
    mounted.current = true; epoch.current += 1;
    return () => { mounted.current = false; epoch.current += 1; };
  }, []);

  // retain publishes one observation and synchronously updates action eligibility; returns nothing.
  // retain 发布单个观测并同步更新操作资格；无返回值。
  const retain = (next: PolicyObservation): void => { retained.current = next; setObservation(next); };

  // refresh reads policy without clearing pending intent or replaying any write; returns nothing.
  // refresh 读取策略，不清除待核查意图或重放写入；无返回值。
  const refresh = async (): Promise<void> => {
    if (busyRef.current || !mounted.current) return;
    // original and token bind this read to its original observer and evidence.
    // original 与 token 将本次读取绑定到原观测者及证据。
    const original = retained.current;
    const token = epoch.current;
    busyRef.current = true; setBusy(true); retain({ ...original, fresh: false, error: "" });
    try {
      // status replaces only the atomic host projection; uncertainty remains until explicit acknowledgement.
      // status 仅替换原子宿主投影；不确定性保留至明确确认。
      const status = await readPluginPrewarmPolicyStatus();
      if (mounted.current && token === epoch.current) retain({ ...original, status, fresh: true, error: "" });
    } catch (reason) {
      if (mounted.current && token === epoch.current) retain({ ...original, fresh: false, error: String(reason) });
    } finally {
      busyRef.current = false;
      if (mounted.current && token === epoch.current) setBusy(false);
    }
  };

  // execute validates the exact observed predecessor or receipt, sends once, then performs only a read.
  // execute 校验精确已观测前驱或回执，发送一次，随后仅执行读取。
  const execute = async (command: PluginPrewarmPolicyCommand): Promise<void> => {
    if (busyRef.current || !mounted.current) return;
    // previous supplies the only authority for this view's explicit mutation.
    // previous 提供本视图明确修改的唯一权威。
    const previous = retained.current;
    if (!previous.fresh || previous.pending || !previous.status) return;
    if (command.kind === "begin") {
      if (previous.status.update || previous.status.capacityUpdateRetained
        || previous.status.runtimeId !== command.request.runtimeId || previous.status.revision !== command.request.expectedRevision) return;
    } else if (!previous.status.update || !samePluginPrewarmPolicyRequest(previous.status.update.request, command.request)
      || !policyReceiptAllows(previous.status.update, command.kind)) return;
    // original copies caller-owned mutable arrays so a later editor change cannot rewrite uncertain intent.
    // original 复制调用方可变数组，防止后续编辑重写未决意图。
    const original = { ...command, request: { ...command.request, rules: command.request.rules.map((rule) => ({ ...rule })) } };
    const token = epoch.current;
    busyRef.current = true; setBusy(true);
    // next remains stale until a full read; write acknowledgement alone cannot authorize another transaction.
    // next 在完整读取前保持陈旧；仅写入确认不能授权另一事务。
    let next: PolicyObservation = { ...previous, fresh: false, pending: original, acknowledged: null, error: "" };
    retain(next);
    try {
      // receipt is retained independently from the following read and can belong to a historical parent.
      // receipt 独立于后续读取保留，且可以属于历史父级。
      const receipt = await executePluginPrewarmPolicyCommand(original);
      if (!mounted.current || token !== epoch.current) return;
      next = { ...next, pending: null, acknowledged: receipt }; retain(next);
      const status = await readPluginPrewarmPolicyStatus();
      if (mounted.current && token === epoch.current) retain({ ...next, status, fresh: true });
    } catch (reason) {
      if (mounted.current && token === epoch.current) retain({ ...next, error: String(reason) });
    } finally {
      busyRef.current = false;
      if (mounted.current && token === epoch.current) setBusy(false);
    }
  };

  // acknowledgeUncertainty clears only local tracking after a full read; it cannot cancel or retry host work.
  // acknowledgeUncertainty 仅在完整读取后清除本地追踪；不能取消或重试宿主工作。
  const acknowledgeUncertainty = (): void => {
    if (!mounted.current || busyRef.current || !retained.current.fresh) return;
    retain({ ...retained.current, pending: null });
  };
  return { observation, busy, refresh, execute, acknowledgeUncertainty };
}
