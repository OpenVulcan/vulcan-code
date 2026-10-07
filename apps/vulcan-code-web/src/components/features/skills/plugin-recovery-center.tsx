import * as React from "react";
import { ArrowUpCircle, CircleCheck, Flame, PackageMinus, SlidersHorizontal } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { StreamPlaceholder } from "@/components/ui/stream-placeholder";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { PageEmptyState, PageItem, PageItemList, PageNotice } from "@/components/page";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginPendingActionOffer, PluginPendingKind, PluginPendingOperation } from "@/service/plugin-pending-operations-service";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-contract";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-contract";
import type { PluginMaintenanceGate } from "./plugin-maintenance-workspace";
import { TechnicalDetails } from "./plugin-maintenance-section";
import { capacityActionLabels, capacityPurposeLabels, capacityReceiptLabels, prewarmActionLabels, prewarmReceiptLabels } from "./plugin-policy-labels";
import type { SkillActionProps } from "./skill-area-types";

// RecoveryKind names one family of retained plugin operations shown by the recovery center.
// RecoveryKind 命名恢复中心显示的一类保留插件操作。
export type RecoveryKind = PluginPendingKind;

// The uncertain foreground identity is observation only, never a second service ownership ledger.
// 不确定前台身份仅是观测，绝不是第二套服务所有权账本。
export interface PluginUninstallUncertainty {
  // pluginId is captured before dispatching the original uninstall.
  // pluginId 在派发原卸载前捕获。
  pluginId: string;
  // requestId is absent when the read-only observation cannot identify the actual original.
  // 只读观测无法标识实际原实例时，requestId 为空。
  requestId: string | null;
}

// Uncertainty is an operation whose last continuation outcome is unknown; null fields match any observed value.
// Uncertainty 是最近一次继续结果未知的操作；空字段匹配任意观测值。
interface Uncertainty {
  kind: PluginPendingKind;
  pluginId: string | null;
  operationId: string | null;
}

// Confirmation freezes one reviewed operation and the exact offer the user is about to accept.
// Confirmation 固定单个已审阅操作及用户即将接受的精确可用动作。
interface Confirmation {
  operation: PluginPendingOperation;
  offer: PluginPendingActionOffer;
}

// PluginRecoveryCenterProps binds the unified registry, the shared gate and the scan trigger.
// PluginRecoveryCenterProps 绑定统一登记表、共享门与扫描触发。
interface PluginRecoveryCenterProps {
  // pluginName resolves display names; plugins absent from the catalog fall back to their ID.
  // pluginName 解析显示名称；不在目录中的插件回退为其 ID。
  pluginName: (pluginId: string) => string;
  // actions are the only read and continuation entries; the center derives no eligibility itself.
  // actions 是唯一读取与继续入口；中心自身不推导任何资格。
  actions: Pick<SkillActionProps, "onReadPluginPendingOperations" | "onContinuePluginPendingOperation">;
  // gate is the page mutation gate shared with the plugin lifecycle controls of the same surface.
  // gate 是与同一界面插件生命周期控件共享的页面变更门。
  gate: PluginMaintenanceGate;
  // kinds limits the displayed operation families; the full read and its checks are unchanged.
  // kinds 限制显示的操作类别；全量读取及其校验不变。
  kinds: ReadonlyArray<RecoveryKind>;
  // scopePluginId restricts the displayed rows to one plugin when opened from its card.
  // scopePluginId 从插件卡片打开时将显示行限定到单个插件。
  scopePluginId?: string;
  // scanToken changes when the user asks to check again; the mount read is the first scan.
  // scanToken 在用户要求重新检查时变化；挂载读取即首次扫描。
  scanToken: number;
  // uninstall bindings preserve the unconfirmed foreground uninstall and the panel's attention bar.
  // 卸载绑定保留未确认的前台卸载及面板提醒条。
  uninstallUncertainty: PluginUninstallUncertainty | null;
  onUninstallConfirmed: (pluginId: string) => void;
  onUninstallRecordsObserved?: (records: PluginUninstallStatus[]) => void;
  // onLocate shows which conversations still use resources held by one original request.
  // onLocate 显示哪些会话仍在使用某个原请求持有的资源。
  onLocate?: (kind: "update" | "uninstall", pluginId: string, requestId: string) => void;
}

// UpdatePhase includes the journal-less reservation, represented by a null service phase.
// UpdatePhase 包含无日志预留，由服务端 null 阶段表示。
type UpdatePhase = NonNullable<LocalPluginUpdateStatus["phase"]> | "reserving";

// updatePhaseLabels names each durable stage as a short status; updatePhaseHints explains it and the way forward.
// updatePhaseLabels 以简短状态命名各持久阶段；updatePhaseHints 解释该阶段及后续做法。
const updatePhaseLabels: Record<UpdatePhase, TranslationKey> = {
  reserving: "pluginUpdates.reservingPreparation", preparing: "pluginUpdates.preparing",
  prepared: "pluginUpdates.prepared", committing: "pluginUpdates.committing", published: "pluginUpdates.published",
  completed: "pluginUpdates.completed", aborted: "pluginUpdates.aborted",
};
const updatePhaseHints: Record<UpdatePhase, TranslationKey> = {
  reserving: "pluginUpdates.hint.reserving", preparing: "pluginUpdates.hint.preparing",
  prepared: "pluginUpdates.hint.prepared", committing: "pluginUpdates.hint.committing", published: "pluginUpdates.hint.published",
  completed: "pluginUpdates.hint.completed", aborted: "pluginUpdates.hint.aborted",
};

// effectLabels distinguishes the exact recorded operation without implying automatic replay.
// effectLabels 区分精确记录操作，不暗示自动重放。
const effectLabels: Record<LocalPluginUpdateStatus["effects"][number]["operation"], TranslationKey> = {
  drain_predecessor_lua: "pluginUpdates.drainPredecessorLua",
  disable_predecessor: "pluginUpdates.disablePredecessor",
  restore_predecessor: "pluginUpdates.restorePredecessor",
  probe: "pluginUpdates.probe", enable: "pluginUpdates.enable", enable_rollback: "pluginUpdates.enableRollback",
};

// matches reports whether operation is the one uncertainty refers to.
// matches 报告 operation 是否为 uncertainty 所指操作。
function matches(operation: PluginPendingOperation, uncertainty: Uncertainty): boolean {
  return operation.kind === uncertainty.kind && (uncertainty.pluginId === null || operation.pluginId === uncertainty.pluginId)
    && (uncertainty.operationId === null || operation.operationId === uncertainty.operationId);
}

// needsDialog reports whether the offer is reviewed in a dialog first; only update abandonment and uninstall continuation are.
// needsDialog 报告可用动作是否需先在对话框中审阅；仅更新放弃与卸载继续需要。
function needsDialog(operation: PluginPendingOperation, offer: PluginPendingActionOffer): boolean {
  return (operation.kind === "update" && offer.action === "abort") || operation.kind === "uninstall";
}

// PluginRecoveryCenter lists every retained plugin operation from the unified registry and continues one at a time.
// PluginRecoveryCenter 从统一登记表列出所有保留插件操作，并逐个继续。
// Each row offers only the continuations the service declared; nothing runs until the user clicks.
// 每行仅提供服务声明的继续方式；用户点击前不执行任何操作。
export function PluginRecoveryCenter(props: PluginRecoveryCenterProps) {
  const { t } = useI18n();
  // operations is the last successful full read; null until the first read settles.
  // operations 是最近一次成功全量读取；首次读取完成前为空。
  const [operations, setOperations] = React.useState<PluginPendingOperation[] | null>(null);
  // fresh permits continuation only after a successful matching observation.
  // fresh 仅在成功匹配观测后允许继续。
  const [fresh, setFresh] = React.useState(false);
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // busyRef closes repeat-click admission before React commits the busy state.
  // busyRef 在 React 提交忙碌状态前关闭重复点击入场。
  const busyRef = React.useRef(false);
  // uncertain survives failed reads; gone records that a successful read no longer lists it.
  // uncertain 跨失败读取保留；gone 记录成功读取已不再列出它。
  const [uncertain, setUncertain] = React.useState<Uncertainty | null>(() => props.uninstallUncertainty === null ? null
    : { kind: "uninstall", pluginId: props.uninstallUncertainty.pluginId, operationId: props.uninstallUncertainty.requestId });
  const [gone, setGone] = React.useState(false);
  // finished records a confirmed successful continuation so the user sees its outcome.
  // finished 记录已确认成功的继续，使用户看到其结果。
  const [finished, setFinished] = React.useState<Confirmation | null>(null);
  const [confirmation, setConfirmation] = React.useState<Confirmation | null>(null);
  // acknowledgement starts false for each reviewed uninstall and is never inferred from its phase.
  // acknowledgement 对每个审阅卸载初始为假，绝不从阶段推断。
  const [acknowledgement, setAcknowledgement] = React.useState(false);
  // error keeps continuation and readback failures visible until the next explicit action.
  // error 保持继续与回读失败可见，直至下一次明确操作。
  const [error, setError] = React.useState("");
  // version invalidates older reads; alive suppresses writes after disposal without cancelling service work.
  // version 使旧读取失效；alive 在销毁后抑制写入，不取消服务工作。
  const version = React.useRef(0);
  const alive = React.useRef(true);

  React.useEffect(() => {
    if (props.uninstallUncertainty !== null) {
      version.current += 1; setFresh(false); setConfirmation(null); setGone(false);
      setUncertain({ kind: "uninstall", pluginId: props.uninstallUncertainty.pluginId, operationId: props.uninstallUncertainty.requestId });
    }
  }, [props.uninstallUncertainty]);

  // load reads the whole registry; an uncertain operation is resolved only by seeing it again or by explicit dismissal.
  // load 读取整个登记表；不确定操作仅在再次看到它或明确放弃追踪时解决。
  // expected is the operation to look for; token rejects stale reads; returns without treating absence as success.
  // expected 是要查找的操作；token 拒绝过期读取；返回时不将缺失当作成功。
  const load = async (expected: Uncertainty | null, token: number): Promise<void> => {
    const observed = await props.actions.onReadPluginPendingOperations();
    const matching = expected === null ? [] : observed.filter((operation) => matches(operation, expected));
    if (matching.length > 1) throw new Error(t("pluginRecovery.identityMismatch"));
    if (alive.current && version.current === token) {
      setOperations(observed); setFresh(true);
      if (expected !== null) { setGone(matching.length === 0); if (matching.length === 1) setUncertain(null); }
      props.onUninstallRecordsObserved?.(observed.flatMap((operation) => operation.kind === "uninstall" ? [operation.detail] : []));
    }
  };

  // read performs one explicit full observation and never retries an operation.
  // read 执行单次明确全量观测，绝不重试操作。
  const read = async (): Promise<void> => {
    if (busyRef.current) return;
    const token = ++version.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null); setFinished(null);
    try { await load(uncertain, token); }
    catch (reason) { if (alive.current && version.current === token) setError(String(reason)); }
    finally { if (version.current === token) { busyRef.current = false; setBusy(null); } }
  };

  // scanRef lets a later scan reuse the latest reader while re-running only when the token changes.
  // scanRef 使后续扫描复用最新读取器，同时仅在令牌变化时重新运行。
  const scanRef = React.useRef({ read, token: props.scanToken });
  React.useEffect(() => { scanRef.current.read = read; });
  React.useEffect(() => {
    // The mount read below is the first scan; only a changed token reads again.
    // 下方挂载读取即首次扫描；仅令牌变化时再次读取。
    if (scanRef.current.token === props.scanToken) return;
    scanRef.current.token = props.scanToken;
    void scanRef.current.read();
  }, [props.scanToken]);

  // mountRef carries the first read's inputs into the mount-only effect.
  // mountRef 将首次读取的输入带入仅挂载效果。
  const mountRef = React.useRef({ load, uncertain });
  React.useEffect(() => {
    alive.current = true;
    // disposed belongs to this mount effect, including React's development remount probe.
    // disposed 属于此挂载效果，包括 React 的开发重挂载探测。
    let disposed = false;
    const token = ++version.current;
    busyRef.current = true; setBusy("read"); setFresh(false);
    void mountRef.current.load(mountRef.current.uncertain, token).catch((reason: unknown) => {
      if (!disposed && alive.current && version.current === token) setError(String(reason));
    }).finally(() => { if (!disposed) { busyRef.current = false; if (alive.current) setBusy(null); } });
    return () => { disposed = true; alive.current = false; version.current += 1; busyRef.current = false; };
  }, []);

  // execute continues exactly one reviewed operation after obtaining the shared synchronous page admission.
  // execute 取得共享同步页面入场后，精确继续单个已审阅操作。
  // Returns after the continuation and a readback; an unknown outcome stays tracked until seen again or dismissed.
  // 继续与回读后返回；未知结果保持追踪，直至再次看到或放弃追踪。
  const execute = async (operation: PluginPendingOperation, offer: PluginPendingActionOffer): Promise<void> => {
    const acknowledged = offer.requiresAcknowledgement && (operation.kind !== "uninstall" || acknowledgement);
    if (busyRef.current || !fresh || uncertain !== null || props.gate.mutationBusy || !operations?.includes(operation)
      || !operation.actions.includes(offer) || (offer.requiresAcknowledgement && !acknowledged) || !props.gate.acquireMutation()) return;
    const token = ++version.current;
    // target is what must reappear when the foreground response is lost.
    // target 是前台响应丢失时必须再次出现的操作。
    const target: Uncertainty = { kind: operation.kind, pluginId: operation.pluginId, operationId: operation.operationId };
    const failures: string[] = [];
    let succeeded = false;
    busyRef.current = true; setBusy("mutation"); setFresh(false); setUncertain(target); setGone(false); setFinished(null); setError("");
    try {
      try {
        await props.actions.onContinuePluginPendingOperation(continuation(operation, offer, acknowledged));
        succeeded = true;
        if (operation.kind === "uninstall") props.onUninstallConfirmed(operation.pluginId);
        if (alive.current && version.current === token) { setFinished({ operation, offer }); setUncertain(null); }
      } catch (reason) { failures.push(String(reason)); }
      try { await load(succeeded ? null : target, token); }
      catch (reason) { failures.push(String(reason)); }
      if (alive.current && version.current === token) { setError(failures.join("\n")); setConfirmation(null); }
    } finally {
      props.gate.releaseMutation();
      if (version.current === token) { busyRef.current = false; setBusy(null); }
    }
  };

  // dismiss stops tracking an operation the registry no longer lists; it neither cancels nor retries service work.
  // dismiss 停止追踪登记表已不再列出的操作；既不取消也不重试服务工作。
  const dismiss = (): void => {
    if (uncertain === null || !gone || busyRef.current) return;
    if (uncertain.kind === "uninstall" && uncertain.pluginId !== null) props.onUninstallConfirmed(uncertain.pluginId);
    setUncertain(null); setGone(false);
  };

  // nameOf renders the operation's owner; the cross-plugin prewarm policy uses its section title.
  // nameOf 渲染操作所有者；跨插件预热策略使用其区块标题。
  const nameOf = (pluginId: string | null): string => pluginId === null ? t("pluginMaintenance.prewarm.title") : props.pluginName(pluginId);
  const canWrite = fresh && busy === null && !props.gate.mutationBusy && uncertain === null;
  const visibleOperations = (operations ?? []).filter((operation) => props.kinds.includes(operation.kind)
    && (props.scopePluginId === undefined || operation.pluginId === props.scopePluginId));
  const showUncertain = uncertain !== null && busy === null;
  const visible = visibleOperations.length + (showUncertain ? 1 : 0) + (finished ? 1 : 0) + (error ? 1 : 0);
  const loading = operations === null && !error;

  // offerButton renders one service-declared continuation with its kind-specific label.
  // offerButton 以类别专属标签渲染单个服务声明的继续方式。
  const offerButton = (operation: PluginPendingOperation, offer: PluginPendingActionOffer): React.ReactNode => {
    const [label, variant] = offerLabel(operation, offer);
    return <Button disabled={!canWrite} key={offer.action} size="sm" type="button" variant={variant}
      onClick={() => { if (needsDialog(operation, offer)) { setConfirmation({ operation, offer }); setAcknowledgement(false); } else void execute(operation, offer); }}>
      {t(label)}</Button>;
  };

  return (
    <>
      {showUncertain && <PageNotice tone="warning" actions={gone ? <Button onClick={dismiss} size="sm" type="button" variant="outline">
        {t("pluginRuntime.discardTracking")}</Button> : undefined}>
        {formatMessage(t(gone ? "pluginRecovery.unconfirmedGone" : uncertain.kind === "uninstall" ? "pluginRecovery.uninstallUnknownNotice" : "pluginUpdates.unconfirmed"),
          { name: nameOf(uncertain.pluginId) })}
      </PageNotice>}
      {error && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{t("pluginRecovery.failed")}{"\n"}{error}</span></PageNotice>}
      {finished && <PageNotice tone="success">{formatMessage(t(finishedLabel(finished)), { name: nameOf(finished.operation.pluginId) })}</PageNotice>}
      <PageItemList>
        {visibleOperations.map((operation) => {
          const row = describe(operation, t, nameOf);
          const locate = props.onLocate !== undefined && (operation.kind === "update" || operation.kind === "uninstall")
            ? () => props.onLocate?.(operation.kind as "update" | "uninstall", operation.pluginId, operation.operationId) : undefined;
          return <PageItem
            actions={<>
              {locate !== undefined && <Button onClick={locate} size="sm" type="button" variant="ghost">{t("pluginDiagnostics.locateRequest")}</Button>}
              {operation.actions.map((offer) => offerButton(operation, offer))}
            </>}
            aria-label={row.title} description={row.description} icon={row.icon} key={`${operation.kind}:${operation.operationId}`}
            meta={row.badge === undefined ? undefined : <Badge variant="secondary">{row.badge}</Badge>} title={row.title}
            tone={operation.actions.length === 0 ? "neutral" : "attention"}>
            <TechnicalDetails entries={row.details} />
          </PageItem>;
        })}
      </PageItemList>
      {loading && visible === 0 && <StreamPlaceholder count={2} label={t("pluginRecovery.scanning")} shape="rows" />}
      {!loading && visible === 0 && <PageEmptyState description={t(props.scopePluginId === undefined ? "pluginRecovery.allClearHint" : "pluginRecovery.allClearScopedHint")}
        icon={<CircleCheck />} title={t("pluginRecovery.allClear")} />}
      <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
        <AlertDialogContent>
          {confirmation && <ConfirmationBody acknowledgement={acknowledgement} busy={busy !== null} confirmation={confirmation}
            name={nameOf(confirmation.operation.pluginId)} onAcknowledgement={setAcknowledgement} />}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={!canWrite || (confirmation?.operation.kind === "uninstall" && confirmation.offer.requiresAcknowledgement && !acknowledgement)}
              onClick={(event) => { event.preventDefault(); if (confirmation) void execute(confirmation.operation, confirmation.offer); }}>
              {t("skills.plugins.confirm")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

// continuation builds the exact wire request for offer on operation; acknowledged is already reviewed by the user.
// continuation 为 operation 上的 offer 构建精确线请求；acknowledged 已由用户审阅。
function continuation(operation: PluginPendingOperation, offer: PluginPendingActionOffer, acknowledged: boolean) {
  const { operationId } = operation;
  switch (operation.kind) {
    case "update":
      return { kind: "update", operationId, pluginId: operation.pluginId, action: offer.action as "resume" | "abort", acknowledgeExternalEffects: acknowledged } as const;
    case "uninstall":
      return { kind: "uninstall", operationId, pluginId: operation.pluginId, action: "resume", acknowledgeExternalEffects: acknowledged } as const;
    default:
      return { kind: operation.kind, operationId, action: offer.action as "resume" | "cancel" | "forget" };
  }
}

// offerLabel names one offer for its operation kind and phase, with the button variant.
// offerLabel 按操作类别与阶段命名单个可用动作，并给出按钮样式。
function offerLabel(operation: PluginPendingOperation, offer: PluginPendingActionOffer): readonly [TranslationKey, "ghost" | "default" | "outline"] {
  switch (operation.kind) {
    case "update": {
      const phase = operation.detail.phase;
      if (offer.action === "abort") return [phase === "aborted" ? "pluginUpdates.finishCleanup" : "pluginUpdates.abandon", "outline"];
      return [phase === "completed" ? "pluginUpdates.finishCleanup" : phase === null || phase === "preparing" ? "pluginUpdates.resumePreparation" : "pluginUpdates.resume", "default"];
    }
    case "uninstall":
      return ["pluginRecovery.uninstallContinue", "default"];
    case "capacityPolicy":
      return capacityActionLabels[offer.action as "resume" | "cancel" | "forget"];
    case "prewarmPolicy":
      return prewarmActionLabels[offer.action as "resume" | "cancel" | "forget"];
  }
}

// finishedLabel names the confirmed outcome of a successful continuation.
// finishedLabel 命名成功继续的已确认结果。
function finishedLabel({ operation, offer }: Confirmation): TranslationKey {
  if (operation.kind === "update") return offer.action === "abort" ? "pluginUpdates.finishedAbort" : "pluginUpdates.finishedResume";
  if (operation.kind === "uninstall") return "pluginRecovery.uninstallConfirmed";
  return "pluginRecovery.policyFinished";
}

// RowView is the rendered shape of one operation row.
// RowView 是单个操作行的渲染形态。
interface RowView {
  icon: React.ReactNode;
  title: string;
  description: string;
  badge?: string;
  details: ReadonlyArray<readonly [string, React.ReactNode]>;
}

// describe renders one operation's plain-language row; raw identifiers stay in the technical details.
// describe 渲染单个操作的平实行；原始标识保留在技术详情中。
function describe(operation: PluginPendingOperation, t: (key: TranslationKey) => string, nameOf: (pluginId: string | null) => string): RowView {
  const request: readonly [string, React.ReactNode] = [t("pluginUpdates.request"), <code key="id">{operation.operationId}</code>];
  switch (operation.kind) {
    case "update": {
      const record = operation.detail;
      const phase: UpdatePhase = record.phase ?? "reserving";
      return {
        icon: <ArrowUpCircle size={16} />, badge: t(updatePhaseLabels[phase]), description: t(updatePhaseHints[phase]),
        title: formatMessage(t("pluginUpdates.itemTitle"), { name: nameOf(operation.pluginId), version: record.candidate.version }),
        details: [request,
          [t("pluginUpdates.installation"), record.installation === null ? t("pluginUpdates.noReceipt")
            : record.installation === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")],
          ...record.effects.map((effect) => [t(effectLabels[effect.operation]), effect.phase === "started" ? t("pluginUpdates.effectStarted")
            : effect.phase === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")] as const)],
      };
    }
    case "uninstall": {
      const executing = operation.detail.phase === "executing";
      return {
        icon: <PackageMinus size={16} />, badge: t(executing ? "pluginRecovery.executing" : "pluginRecovery.pending"),
        description: t(executing ? "pluginRecovery.hint.executing" : "pluginRecovery.hint.uninstall"),
        title: formatMessage(t("pluginRecovery.uninstallItemTitle"), { name: nameOf(operation.pluginId) }),
        details: [[t("pluginUpdates.plugin"), <code key="plugin">{operation.pluginId}</code>], request,
          [t("pluginRecovery.phase"), <code key="phase">{operation.detail.phase}</code>]],
      };
    }
    case "capacityPolicy": {
      const receipt = operation.detail;
      return {
        icon: <SlidersHorizontal size={16} />, description: [receipt.error, receipt.persistenceError].filter(Boolean).join("\n"),
        title: formatMessage(t(capacityReceiptLabels[receipt.phase]), { name: nameOf(operation.pluginId), purpose: t(capacityPurposeLabels[receipt.request.purpose]) }),
        details: [request, [t("pluginRecovery.phase"), <code key="phase">{receipt.phase}</code>]],
      };
    }
    case "prewarmPolicy": {
      const receipt = operation.detail;
      return {
        icon: <Flame size={16} />, description: [receipt.error, receipt.persistenceError].filter(Boolean).join("\n"),
        title: t(prewarmReceiptLabels[receipt.phase]),
        details: [request, [t("pluginRecovery.phase"), <code key="phase">{receipt.phase}</code>]],
      };
    }
  }
}

// ConfirmationBody explains the reviewed continuation; only an uninstall that requires it shows the acknowledgement box.
// ConfirmationBody 说明已审阅的继续；仅要求确认的卸载显示确认勾选框。
function ConfirmationBody(props: { confirmation: Confirmation; name: string; acknowledgement: boolean; busy: boolean; onAcknowledgement: (value: boolean) => void }) {
  const { t } = useI18n();
  const { operation, offer } = props.confirmation;
  const cleanup = operation.kind === "update" && operation.detail.phase === "aborted";
  const [title, warning, item]: readonly [TranslationKey, TranslationKey, string] = operation.kind === "uninstall"
    ? ["pluginRecovery.uninstallContinue", "pluginRecovery.uninstallWarning", formatMessage(t("pluginRecovery.uninstallItemTitle"), { name: props.name })]
    : [cleanup ? "pluginUpdates.finishCleanup" : "pluginUpdates.abandon", cleanup ? "pluginUpdates.cleanupWarning" : "pluginUpdates.abandonWarning",
      operation.kind === "update" ? formatMessage(t("pluginUpdates.itemTitle"), { name: props.name, version: operation.detail.candidate.version }) : props.name];
  return <>
    <AlertDialogHeader><AlertDialogTitle>{t(title)}</AlertDialogTitle><AlertDialogDescription>{t(warning)}</AlertDialogDescription></AlertDialogHeader>
    <p className="text-sm">{item}</p>
    <TechnicalDetails entries={[[t("pluginUpdates.request"), <code key="id">{operation.operationId}</code>]]} />
    {operation.kind === "uninstall" && offer.requiresAcknowledgement && <label className="flex items-start gap-2 text-sm">
      <Checkbox checked={props.acknowledgement} disabled={props.busy} onCheckedChange={(checked) => props.onAcknowledgement(checked === true)} />
      {t("pluginRecovery.uninstallAcknowledgement")}
    </label>}
  </>;
}
