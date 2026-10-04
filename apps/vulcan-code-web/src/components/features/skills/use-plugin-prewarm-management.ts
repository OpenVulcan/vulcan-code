import * as React from "react";
import { executePluginPrewarmCommand, readPluginPrewarmCatalog, type PluginPrewarmCommand } from "@/service/plugin-prewarm-service";
import type { PluginPrewarmCatalog, PluginPrewarmOwner, PluginPrewarmReceipt } from "@/types/plugin-prewarm";

// PrewarmObservation keeps one exact owner's evidence separate from freshness and uncertain commands.
// PrewarmObservation 将单个精确归属的证据与新鲜度、不确定操作分离。
interface PrewarmObservation {
  // catalog is the last validated response, including acknowledgements retained after a read failure.
  // catalog 为最后校验响应，包含读取失败后保留的确认回执。
  catalog: PluginPrewarmCatalog | null;
  // fresh is revoked before every network operation and restored only by a complete catalog read.
  // fresh 在每次网络操作前撤销，仅由完整目录读取恢复。
  fresh: boolean;
  // pending is the original uncertain command; it is never automatically retried.
  // pending 为结果不确定的原操作，绝不自动重试。
  pending: PluginPrewarmCommand | null;
  // error reports observation or transport failures separately from native receipt errors.
  // error 将观测或传输失败与原生回执错误分开报告。
  error: string;
}

// emptyObservation authorizes no mutation before the user's first successful read.
// emptyObservation 在用户首次成功读取前不授权修改。
const emptyObservation: PrewarmObservation = { catalog: null, fresh: false, pending: null, error: "" };

// scopeKey returns a collision-free key for the declared pair, including arbitrary opaque parent strings.
// scopeKey 为声明身份组合返回无碰撞键，支持任意不透明父级字符串。
function scopeKey(owner: PluginPrewarmOwner): string { return JSON.stringify([owner.runtimeId, owner.ownerSessionId]); }

// usePluginPrewarmManagement retains separate scopes across selection changes and returns explicit actions.
// usePluginPrewarmManagement 跨选择变化保留独立范围，并返回明确操作。
// owner selects the visible scope only; mounting or changing it never initializes Lua.
// owner 仅选择可见范围；挂载或改变选择绝不初始化 Lua。
export function usePluginPrewarmManagement(owner: PluginPrewarmOwner | null) {
  // observations and retained share one immutable Map snapshot for rendering and synchronous event guards.
  // observations 与 retained 共用不可变 Map 快照，用于渲染及同步事件保护。
  const [observations, setObservations] = React.useState<ReadonlyMap<string, PrewarmObservation>>(new Map());
  const retained = React.useRef(observations);
  const [busy, setBusy] = React.useState(false);
  const busyRef = React.useRef(false);
  // mounted and lifecycle reject completions from an unmounted or StrictMode-discarded observer.
  // mounted 与 lifecycle 拒绝已卸载或 StrictMode 丢弃观测者的完成结果。
  const mounted = React.useRef(false);
  const lifecycle = React.useRef(0);
  React.useEffect(() => {
    mounted.current = true;
    lifecycle.current += 1;
    return () => { mounted.current = false; lifecycle.current += 1; };
  }, []);

  // retain publishes a new map for key without letting one owner's late response replace another's state.
  // retain 为 key 发布新映射，防止某归属的迟到响应替换其他归属的状态。
  const retain = (key: string, entry: PrewarmObservation): void => {
    // next changes only the original scope and is also the authority for same-tick handlers.
    // next 仅改变原范围，同时作为同一时刻处理器的权威。
    const next = new Map(retained.current);
    next.set(key, entry); retained.current = next; setObservations(next);
  };

  // refresh queries the selected scope without replaying its pending preparation; returns nothing.
  // refresh 查询选中范围，不重放待核查准备；无返回值。
  const refresh = async (): Promise<void> => {
    if (!owner || busyRef.current || !mounted.current) return;
    // key and token freeze the original scope and observer for this request.
    // key 与 token 冻结本次请求的原范围及观测者。
    const key = scopeKey(owner);
    const token = lifecycle.current;
    const previous = retained.current.get(key) ?? emptyObservation;
    busyRef.current = true; setBusy(true); retain(key, { ...previous, fresh: false, error: "" });
    try {
      // catalog is validated by the service against the original owner before reaching this hook.
      // catalog 在进入本钩子前已由服务层对照原归属校验。
      const catalog = await readPluginPrewarmCatalog(owner);
      if (mounted.current && token === lifecycle.current) retain(key, { ...previous, catalog, fresh: true, error: "" });
    } catch (reason) {
      if (mounted.current && token === lifecycle.current) retain(key, { ...previous, fresh: false, error: String(reason) });
    } finally {
      busyRef.current = false;
      if (mounted.current && token === lifecycle.current) setBusy(false);
    }
  };

  // execute sends a reviewed command once, retains uncertainty, then only reads after acknowledgement.
  // execute 发送一次已审阅操作，保留不确定结果，确认后仅执行读取。
  const execute = async (command: PluginPrewarmCommand): Promise<void> => {
    if (!owner || busyRef.current || !mounted.current || scopeKey(command.request) !== scopeKey(owner)) return;
    // previous must be fresh and unambiguous before a new explicit mutation is admitted.
    // 接受新的明确修改之前，previous 必须新鲜且没有未确认操作。
    const key = scopeKey(owner);
    const previous = retained.current.get(key) ?? emptyObservation;
    if (!previous.fresh || previous.pending || !previous.catalog) return;
    const token = lifecycle.current;
    busyRef.current = true; setBusy(true); retain(key, { ...previous, fresh: false, pending: command, error: "" });
    // acknowledged preserves a successful receipt even when the following read fails.
    // acknowledged 在后续读取失败时仍保留成功确认回执。
    let acknowledged = { ...previous, fresh: false, pending: command as PluginPrewarmCommand | null, error: "" };
    try {
      // receipt is null only for an acknowledged forget; otherwise its exact original ID is merged.
      // receipt 仅在确认遗忘时为空；其他情况合并精确原始 ID。
      const receipt = await executePluginPrewarmCommand(command);
      if (!mounted.current || token !== lifecycle.current) return;
      // targets changes only the acknowledged receipt, never other retained work.
      // targets 仅改变已确认回执，绝不改变其他保留工作。
      let targets: PluginPrewarmReceipt[];
      if (receipt !== null) {
        targets = previous.catalog.targets.filter((target) => target.invocationId !== receipt.invocationId);
        targets.push(receipt);
      } else {
        if (command.kind !== "forget") throw new Error("prewarm: missing acknowledgement receipt");
        targets = previous.catalog.targets.filter((target) => target.invocationId !== command.request.invocationId);
      }
      acknowledged = { catalog: { ...previous.catalog, targets }, fresh: false, pending: null, error: "" };
      retain(key, acknowledged);
      const catalog = await readPluginPrewarmCatalog(owner);
      if (mounted.current && token === lifecycle.current) retain(key, { ...acknowledged, catalog, fresh: true });
    } catch (reason) {
      if (mounted.current && token === lifecycle.current) retain(key, { ...acknowledged, error: String(reason) });
    } finally {
      busyRef.current = false;
      if (mounted.current && token === lifecycle.current) setBusy(false);
    }
  };

  // acknowledgeUncertainty clears this view's warning after a successful read; it never cancels host work.
  // acknowledgeUncertainty 在成功读取后清除本视图提示；绝不取消宿主工作。
  const acknowledgeUncertainty = (): void => {
    if (!owner || busyRef.current) return;
    // previous remains the original scope, even when other sessions are visible elsewhere.
    // 即使其他位置显示不同会话，previous 仍保持原范围。
    const key = scopeKey(owner);
    const previous = retained.current.get(key);
    if (previous?.fresh) retain(key, { ...previous, pending: null });
  };

  // history remains visible after parent replacement but can never authorize commands in the new runtime.
  // history 在父级替换后保持可见，但绝不能授权新运行时中的操作。
  const history = [...observations.values()].filter((entry) => entry.catalog !== null
    && entry.catalog.runtimeId !== owner?.runtimeId && (entry.catalog.targets.length > 0 || entry.pending !== null));
  return { observation: owner ? observations.get(scopeKey(owner)) ?? emptyObservation : emptyObservation,
    history, busy, refresh, execute, acknowledgeUncertainty };
}
