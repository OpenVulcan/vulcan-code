import * as React from "react";
import { ArrowUpCircle } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-service";
import type { SkillActionProps } from "./skill-area-types";
import { PageItem, PageNotice } from "@/components/page";
import { TechnicalDetails, useRecoveryReport, type RecoveryReport } from "./plugin-maintenance-section";

// PluginUpdateControlProps binds one plugin's retained update requests to the existing service action chain.
// PluginUpdateControlProps 将单个插件的保留更新请求绑定到既有服务动作链。
interface PluginUpdateControlProps {
  // pluginId is the exact owner checked by this reader; it never changes for one mounted reader.
  // pluginId 是此读取器检查的精确所有者；单个已挂载读取器中不会改变。
  pluginId: string;
  // pluginName is the localized display name used in plain-language titles.
  // pluginName 是用于平实标题的本地化显示名称。
  pluginName: string;
  // actions preserve original request identity and apply the returned catalog through the ordinary publisher.
  // actions 保留原请求身份，并通过普通发布器应用返回目录。
  actions: Pick<SkillActionProps, "onReadLocalPluginUpdates" | "onResumeLocalPluginUpdate" | "onAbortLocalPluginUpdate">;
  // onLocateDiagnostics shows what still holds the exact retained request, without executing recovery.
  // onLocateDiagnostics 显示仍占用精确保留请求的资源，不执行恢复。
  onLocateDiagnostics?: (pluginId: string, requestId: string) => void;
  // scanToken changes whenever the container asks every reader to check again.
  // scanToken 在容器要求所有读取器重新检查时变化。
  scanToken: number;
  // onReport tells the container whether anything is shown, so it can display an all-clear state.
  // onReport 告知容器是否有显示内容，使其可显示一切正常状态。
  onReport?: (report: RecoveryReport) => void;
}

// UpdatePhase includes the journal-less reservation, represented by a null service phase.
// UpdatePhase 包含无日志预留，由服务端 null 阶段表示。
type UpdatePhase = NonNullable<LocalPluginUpdateStatus["phase"]> | "reserving";

// phaseLabels names each durable stage as a short status; phaseHints explains it and the way forward.
// phaseLabels 以简短状态命名各持久阶段；phaseHints 解释该阶段及后续做法。
const phaseLabels: Record<UpdatePhase, TranslationKey> = {
  reserving: "pluginUpdates.reservingPreparation", preparing: "pluginUpdates.preparing",
  prepared: "pluginUpdates.prepared", committing: "pluginUpdates.committing", published: "pluginUpdates.published",
  completed: "pluginUpdates.completed", aborted: "pluginUpdates.aborted",
};
const phaseHints: Record<UpdatePhase, TranslationKey> = {
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

// canResume reports whether the service accepts continuing this exact retained request.
// canResume 报告服务是否接受继续此精确保留请求。
function canResume(record: LocalPluginUpdateStatus): boolean {
  return ((record.phase === null || record.phase === "preparing") && record.hasServiceOwner)
    || record.phase === "committing" || record.phase === "published"
    || (record.phase === "completed" && record.hasServiceOwner);
}

// canAbandon reports whether the service accepts abandoning, or finishing cleanup of, this exact request.
// canAbandon 报告服务是否接受放弃此精确请求或完成其清理。
function canAbandon(record: LocalPluginUpdateStatus): boolean {
  return (record.phase === null && record.hasServiceOwner) || record.phase === "preparing" || record.phase === "prepared"
    || record.phase === "committing" || (record.phase === "aborted" && record.hasServiceOwner);
}

// PluginUpdateControls checks one plugin for interrupted updates and offers the exact continuation or abandonment.
// PluginUpdateControls 检查单个插件中断的更新，并提供精确的继续或放弃操作。
// Finished history without remaining work is not shown; it needs no decision from the user.
// 已结束且无剩余工作的历史不显示；它不需要用户做任何决定。
export function PluginUpdateControls(props: PluginUpdateControlProps) {
  // observation is the last actual read; fresh gates every mutation on a successful matching read.
  // observation 是最后一次实际读取；fresh 以成功匹配读取为每次变更的前提。
  const [observation, setObservation] = React.useState<LocalPluginUpdateStatus[] | null>(null);
  const [fresh, setFresh] = React.useState(false);
  // error keeps mutation and readback failures visible until the next explicit action.
  // error 保持变更与回读失败可见，直至下一次明确操作。
  const [error, setError] = React.useState("");
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // confirmation retains the exact reviewed request, never a list position.
  // confirmation 保留精确已审阅请求，绝不保留列表位置。
  const [confirmation, setConfirmation] = React.useState<LocalPluginUpdateStatus | null>(null);
  // unconfirmed survives empty rereads until the exact original record is observed again.
  // unconfirmed 跨空回读保留，直至再次观察到精确原记录。
  const [unconfirmed, setUnconfirmed] = React.useState<LocalPluginUpdateStatus | null>(null);
  // finished records a confirmed successful operation so the user sees its outcome.
  // finished 记录已确认成功的操作，使用户看到其结果。
  const [finished, setFinished] = React.useState<"resume" | "abort" | null>(null);
  // busyRef closes repeat-click admission before React commits the busy state.
  // busyRef 在 React 提交忙碌状态前关闭重复点击入场。
  const busyRef = React.useRef(false);
  // versionRef invalidates older reads; aliveRef suppresses writes after disposal without cancelling service work.
  // versionRef 使旧读取失效；aliveRef 在销毁后抑制写入，不取消服务工作。
  const versionRef = React.useRef(0);
  const aliveRef = React.useRef(true);
  const { t } = useI18n();
  const name = props.pluginName;
  React.useEffect(() => {
    aliveRef.current = true;
    // Disposal releases admission so a StrictMode remount can read again; stale replies are discarded by version.
    // 销毁释放入场，使 StrictMode 重新挂载可再次读取；过期回复按版本丢弃。
    return () => { aliveRef.current = false; versionRef.current += 1; busyRef.current = false; };
  }, []);

  // load reads this owner for one observation version; a missing expected original fails without replacing evidence.
  // load 为单个观测版本读取此所有者；预期原记录缺失时失败，不替换证据。
  const load = React.useCallback(async (version: number, expectedRequest?: string): Promise<void> => {
    // records are already validated by the service adapter against the owner.
    // records 已由服务适配器按所有者校验。
    const records = await props.actions.onReadLocalPluginUpdates(props.pluginId);
    if (expectedRequest !== undefined && !records.some((record) => record.requestId === expectedRequest)) {
      throw new Error(t("pluginUpdates.missingOriginal"));
    }
    if (aliveRef.current && versionRef.current === version) {
      setObservation(records); setFresh(true);
      if (expectedRequest !== undefined) setUnconfirmed(null);
    }
  }, [props.actions, props.pluginId, t]);

  // read checks this owner again; a retained uncertain operation requires its original record in the reply.
  // read 再次检查此所有者；保留的不确定操作要求回复包含其原记录。
  const read = React.useCallback(async (expectedRequest: string | undefined): Promise<void> => {
    if (busyRef.current) return;
    const version = ++versionRef.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null); setFinished(null);
    try { await load(version, expectedRequest); }
    catch (reason) { if (aliveRef.current && versionRef.current === version) setError(String(reason)); }
    finally { if (versionRef.current === version) { busyRef.current = false; setBusy(null); } }
  }, [load]);

  // scanRef lets the scan effect use the latest reader and uncertain request while re-running only on a new scan.
  // scanRef 使扫描效果使用最新读取器与不确定请求，同时仅在新扫描时重新运行。
  const scanRef = React.useRef({ read, unconfirmed });
  React.useEffect(() => { scanRef.current = { read, unconfirmed }; });
  React.useEffect(() => {
    void scanRef.current.read(scanRef.current.unconfirmed?.requestId);
  }, [props.scanToken]);

  // execute runs one reviewed action, then re-reads durable state even when the foreground response fails.
  // execute 执行单个已审阅操作，即使前台响应失败也重新读取持久状态。
  const execute = async (kind: "resume" | "abort", record: LocalPluginUpdateStatus): Promise<void> => {
    if (busyRef.current || unconfirmed !== null || !fresh || record.pluginId !== props.pluginId) return;
    const version = ++versionRef.current;
    setUnconfirmed(record);
    busyRef.current = true; setBusy("mutation"); setFresh(false); setError(""); setFinished(null);
    // failures keeps mutation and readback errors separately; a successful read cannot erase the original error.
    // failures 分别保留变更与回读错误；成功读取不能抹去原错误。
    const failures: string[] = [];
    let succeeded = false;
    try {
      if (kind === "resume") await props.actions.onResumeLocalPluginUpdate(record.pluginId, record.requestId);
      else await props.actions.onAbortLocalPluginUpdate(record.pluginId, record.requestId, true);
      succeeded = true;
    } catch (reason) { failures.push(String(reason)); }
    try { await load(version, record.requestId); }
    catch (reason) { failures.push(String(reason)); }
    if (aliveRef.current && versionRef.current === version) {
      setError(failures.join("\n")); setConfirmation(null); setBusy(null);
      setFinished(succeeded && failures.length === 0 ? kind : null);
      busyRef.current = false;
    }
  };

  const canWrite = fresh && busy === null && unconfirmed === null;
  // pending lists only requests that still need a decision; finished history is omitted.
  // pending 仅列出仍需决定的请求；已结束的历史被省略。
  const pending = (observation ?? []).filter((record) => canResume(record) || canAbandon(record));
  // cleanupConfirmation confirms cleanup only for an already abandoned request.
  // cleanupConfirmation 仅对已放弃请求确认清理。
  const cleanupConfirmation = confirmation !== null && confirmation.phase === "aborted" && confirmation.hasServiceOwner;
  const showUnconfirmed = unconfirmed !== null && busy === null;
  const visible = pending.length + (showUnconfirmed ? 1 : 0) + (error ? 1 : 0) + (finished ? 1 : 0);
  useRecoveryReport(props.onReport, observation === null && !error, visible);
  return <>
    {showUnconfirmed && <PageNotice tone="warning">{formatMessage(t("pluginUpdates.unconfirmed"), { name })}</PageNotice>}
    {error && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{formatMessage(t("pluginUpdates.failed"), { name })}{"\n"}{error}</span></PageNotice>}
    {finished && <PageNotice tone="success">{formatMessage(t(finished === "resume" ? "pluginUpdates.finishedResume" : "pluginUpdates.finishedAbort"), { name })}</PageNotice>}
    {pending.map((record) => {
      const phase: UpdatePhase = record.phase ?? "reserving";
      return <PageItem
        actions={<>
          {props.onLocateDiagnostics !== undefined && <Button onClick={() => props.onLocateDiagnostics?.(record.pluginId, record.requestId)} size="sm" type="button" variant="ghost">
            {t("pluginDiagnostics.locateRequest")}</Button>}
          {canAbandon(record) && <Button disabled={!canWrite} onClick={() => setConfirmation(record)} size="sm" type="button" variant="outline">
            {record.phase === "aborted" ? t("pluginUpdates.finishCleanup") : t("pluginUpdates.abandon")}</Button>}
          {canResume(record) && <Button disabled={!canWrite} onClick={() => void execute("resume", record)} size="sm" type="button">
            {record.phase === "completed" ? t("pluginUpdates.finishCleanup") : record.phase === null || record.phase === "preparing"
              ? t("pluginUpdates.resumePreparation") : t("pluginUpdates.resume")}</Button>}
        </>}
        description={<>{t(phaseHints[phase])}</>}
        icon={<ArrowUpCircle size={16} />}
        key={record.requestId}
        aria-label={formatMessage(t("pluginUpdates.itemTitle"), { name, version: record.candidate.version })}
        meta={<Badge variant="secondary">{t(phaseLabels[phase])}</Badge>}
        title={formatMessage(t("pluginUpdates.itemTitle"), { name, version: record.candidate.version })}
        tone="attention"
      >
        <TechnicalDetails entries={[
          [t("pluginUpdates.request"), <code key="id">{record.requestId}</code>],
          [t("pluginUpdates.installation"), record.installation === null ? t("pluginUpdates.noReceipt")
            : record.installation === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")],
          ...record.effects.map((effect) => [t(effectLabels[effect.operation]), effect.phase === "started" ? t("pluginUpdates.effectStarted")
            : effect.phase === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")] as const),
        ]} />
      </PageItem>;
    })}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{cleanupConfirmation ? t("pluginUpdates.finishCleanup") : t("pluginUpdates.abandon")}</AlertDialogTitle>
          <AlertDialogDescription>{cleanupConfirmation ? t("pluginUpdates.cleanupWarning") : t("pluginUpdates.abandonWarning")}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation && <p className="text-sm">{formatMessage(t("pluginUpdates.itemTitle"), { name, version: confirmation.candidate.version })}</p>}
        {confirmation && <TechnicalDetails entries={[[t("pluginUpdates.request"), <code key="id">{confirmation.requestId}</code>]]} />}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={!canWrite} onClick={(event) => {
            event.preventDefault(); if (confirmation) void execute("abort", confirmation);
          }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}
