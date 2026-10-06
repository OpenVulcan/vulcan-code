import * as React from "react";
import { KeyRound } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSecretMutationPhase, PluginSecretMutationStatus } from "@/service/plugin-secret-recovery-service";
import type { SkillActionProps } from "./skill-area-types";
import { PageItem, PageNotice } from "@/components/page";
import { TechnicalDetails, useRecoveryReport, type RecoveryReport } from "./plugin-maintenance-section";

// RecoveryRecord is a non-secret observation of one actual retained service request.
// RecoveryRecord 是单个实际保留服务请求的无秘密观测。
type RecoveryRecord = PluginSecretMutationStatus;

// RecoveryProps binds one plugin's retained credential requests to the shared page mutation gate.
// RecoveryProps 将单个插件的保留凭据请求绑定至共享页面变更门。
interface RecoveryProps {
  // pluginId is the exact owner checked by this reader; pluginName is its display name.
  // pluginId 是此读取器检查的精确所有者；pluginName 是其显示名称。
  pluginId: string;
  pluginName: string;
  // actions forward original plugin and request identifiers without reconstructing physical ownership.
  // actions 原样传递原插件及请求标识，不重建物理所有权。
  actions: Pick<SkillActionProps, "onReadPluginSecretMutations" | "onRecoverPluginSecretMutation">;
  // mutationBusy reflects the parent page's shared lifecycle admission.
  // mutationBusy 反映父页面共享生命周期入场。
  mutationBusy: boolean;
  // acquireMutation returns true only after atomically acquiring the existing page mutation gate.
  // acquireMutation 仅在原子取得既有页面变更门后返回真。
  acquireMutation: () => boolean;
  // releaseMutation releases this same admission after the explicit recovery and readback attempt.
  // releaseMutation 在明确恢复及回读尝试后释放此相同入场。
  releaseMutation: () => void;
  // scanToken changes whenever the container asks every reader to check again.
  // scanToken 在容器要求所有读取器重新检查时变化。
  scanToken: number;
  // onReport tells the container whether anything is shown.
  // onReport 告知容器是否有显示内容。
  onReport?: (report: RecoveryReport) => void;
}

// RecoveryStage groups the sanitized phases into what the recover action will actually do.
// RecoveryStage 将脱敏阶段归类为恢复动作实际要做的事。
type RecoveryStage = "executing" | "rollback" | "cleanup" | "finish";

// stageOf maps one durable phase to the recovery the service will perform for it.
// stageOf 将单个持久阶段映射为服务将执行的恢复。
function stageOf(phase: PluginSecretMutationPhase): RecoveryStage {
  if (phase === "executing") return "executing";
  if (phase === "committed") return "finish";
  if (phase === "restored") return "cleanup";
  return "rollback";
}

// Copy for each stage: the short status, the plain explanation and the action button.
// 各阶段文案：简短状态、平实说明及操作按钮。
const stageLabels: Record<RecoveryStage, TranslationKey> = {
  executing: "pluginRecovery.executing", rollback: "pluginRecovery.pending",
  cleanup: "pluginRecovery.restored", finish: "pluginRecovery.committed",
};
const stageHints: Record<RecoveryStage, TranslationKey> = {
  executing: "pluginRecovery.hint.executing", rollback: "pluginRecovery.hint.secretRollback",
  cleanup: "pluginRecovery.hint.cleanup", finish: "pluginRecovery.hint.secretFinish",
};
const stageActions: Record<Exclude<RecoveryStage, "executing">, TranslationKey> = {
  rollback: "pluginRecovery.action.rollbackSecret", cleanup: "pluginRecovery.action.cleanup", finish: "pluginRecovery.action.finishSecret",
};

// PluginRequestRecoveryControls checks one plugin for interrupted credential changes and offers recovery.
// PluginRequestRecoveryControls 检查单个插件中断的凭据变更并提供恢复。
// Missing ownership is never treated as successful cleanup; an uncertain outcome must be observed again.
// 所有权缺失绝不视为清理成功；不确定结果必须再次被观测到。
export function PluginRequestRecoveryControls(props: RecoveryProps) {
  // observation is the last actual read; fresh permits a mutation only after a successful read.
  // observation 是最后一次实际读取；fresh 仅在成功读取后允许变更。
  const [observation, setObservation] = React.useState<RecoveryRecord[] | null>(null);
  const [fresh, setFresh] = React.useState(false);
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // error retains recovery and subsequent readback failures independently from successful observations.
  // error 独立于成功观测保留恢复及后续回读失败。
  const [error, setError] = React.useState("");
  // confirmation freezes the reviewed original without retaining any credential value.
  // confirmation 固定已审阅原实例，不保留任何凭据值。
  const [confirmation, setConfirmation] = React.useState<RecoveryRecord | null>(null);
  // uncertain retains an unacknowledged foreground outcome across empty rereads.
  // uncertain 跨空回读保留未经确认的前台结果。
  const [uncertain, setUncertain] = React.useState<RecoveryRecord | null>(null);
  // confirmed records an actual successful recovery response even if later readback fails.
  // confirmed 记录实际成功恢复响应，即使后续回读失败。
  const [confirmed, setConfirmed] = React.useState<RecoveryRecord | null>(null);
  const busyRef = React.useRef(false);
  const versionRef = React.useRef(0);
  const aliveRef = React.useRef(true);
  const { t } = useI18n();
  const name = props.pluginName;
  React.useEffect(() => {
    aliveRef.current = true;
    // Disposal releases local admission so a StrictMode remount can read again; stale replies are discarded by version.
    // 销毁释放局部入场，使 StrictMode 重新挂载可再次读取；过期回复按版本丢弃。
    return () => { aliveRef.current = false; versionRef.current += 1; busyRef.current = false; };
  }, []);

  // load observes this owner; requiredOriginal prevents an uncertain disappearance from authorizing another action.
  // load 观测此所有者；requiredOriginal 阻止不确定消失授权另一动作。
  const load = React.useCallback(async (version: number, requiredOriginal: RecoveryRecord | null): Promise<void> => {
    const records = await props.actions.onReadPluginSecretMutations(props.pluginId);
    if (records.some((record) => record.pluginId !== props.pluginId)
      || new Set(records.map((record) => record.requestId)).size !== records.length) {
      throw new Error(t("pluginRecovery.identityMismatch"));
    }
    if (requiredOriginal !== null && !records.some((record) => record.requestId === requiredOriginal.requestId)) {
      throw new Error(t("pluginRecovery.unknown"));
    }
    if (aliveRef.current && versionRef.current === version) {
      setObservation(records); setFresh(true);
      if (requiredOriginal !== null) setUncertain(null);
    }
  }, [props.actions, props.pluginId, t]);

  // read checks this owner again and never starts or retries recovery.
  // read 再次检查此所有者，绝不启动或重试恢复。
  const read = React.useCallback(async (requiredOriginal: RecoveryRecord | null): Promise<void> => {
    if (busyRef.current) return;
    const version = ++versionRef.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null); setConfirmed(null);
    try { await load(version, requiredOriginal); }
    catch (reason) { if (aliveRef.current && versionRef.current === version) setError(String(reason)); }
    finally { if (versionRef.current === version) { busyRef.current = false; setBusy(null); } }
  }, [load]);

  // scanRef lets the scan effect use the latest reader and uncertain record while re-running only on a new scan.
  // scanRef 使扫描效果使用最新读取器与不确定记录，同时仅在新扫描时重新运行。
  const scanRef = React.useRef({ read, uncertain });
  React.useEffect(() => { scanRef.current = { read, uncertain }; });
  React.useEffect(() => {
    void scanRef.current.read(scanRef.current.uncertain);
  }, [props.scanToken]);

  // execute borrows the page gate, recovers the reviewed request once, then observes the same owner on every outcome.
  // execute 借用页面门，一次恢复已审阅请求，然后在每种结果下观测相同所有者。
  const execute = async (record: RecoveryRecord): Promise<void> => {
    if (busyRef.current || !fresh || uncertain !== null || props.mutationBusy || record.phase === "executing"
      || record.pluginId !== props.pluginId || !props.acquireMutation()) return;
    const version = ++versionRef.current;
    // failures retains the foreground failure even if the subsequent read succeeds.
    // failures 即使后续读取成功也保留前台失败。
    const failures: string[] = [];
    let succeeded = false;
    busyRef.current = true; setBusy("mutation"); setFresh(false); setUncertain(record); setConfirmed(null); setError("");
    try {
      try {
        await props.actions.onRecoverPluginSecretMutation(record.pluginId, record.requestId);
        succeeded = true;
        if (aliveRef.current && versionRef.current === version) { setConfirmed(record); setUncertain(null); }
      } catch (reason) { failures.push(String(reason)); }
      try { await load(version, succeeded ? null : record); }
      catch (reason) { failures.push(String(reason)); }
      if (aliveRef.current && versionRef.current === version) { setError(failures.join("\n")); setConfirmation(null); }
    } finally {
      props.releaseMutation();
      if (versionRef.current === version) { busyRef.current = false; setBusy(null); }
    }
  };

  const canWrite = fresh && !busy && !props.mutationBusy && uncertain === null;
  const records = observation ?? [];
  const showUncertain = uncertain !== null && busy === null;
  const visible = records.length + (showUncertain ? 1 : 0) + (confirmed ? 1 : 0) + (error ? 1 : 0);
  useRecoveryReport(props.onReport, observation === null && !error, visible);
  return <>
    {showUncertain && <PageNotice tone="warning">{formatMessage(t("pluginRecovery.unknownNotice"), { name })}</PageNotice>}
    {confirmed && <PageNotice tone="success">{formatMessage(t("pluginRecovery.confirmed"), { name })}</PageNotice>}
    {error && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{formatMessage(t("pluginRecovery.failed"), { name })}{"\n"}{error}</span></PageNotice>}
    {records.map((record) => {
      const stage = stageOf(record.phase);
      const title = formatMessage(t("pluginRecovery.secretItemTitle"), { name });
      return <PageItem
        actions={stage === "executing" ? undefined : <Button disabled={!canWrite} onClick={() => setConfirmation(record)} size="sm" type="button">
          {t(stageActions[stage])}</Button>}
        description={t(stageHints[stage])}
        icon={<KeyRound size={16} />}
        key={record.requestId}
        aria-label={title}
        meta={<Badge variant="secondary">{t(stageLabels[stage])}</Badge>}
        title={title}
        tone={stage === "executing" ? "neutral" : "attention"}
      >
        <TechnicalDetails entries={[
          [t("pluginUpdates.request"), <code key="id">{record.requestId}</code>],
          [t("pluginRecovery.phase"), <code key="phase">{record.phase}</code>],
          [t("pluginRecovery.credential"), <code key="secret">{record.serverId} / {record.secretKey}</code>],
        ]} />
      </PageItem>;
    })}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{confirmation && stageOf(confirmation.phase) !== "executing"
            ? t(stageActions[stageOf(confirmation.phase) as Exclude<RecoveryStage, "executing">]) : t("pluginRecovery.recover")}</AlertDialogTitle>
          <AlertDialogDescription>{t("pluginRecovery.secretWarning")}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation && <p className="text-sm">{formatMessage(t("pluginRecovery.secretItemTitle"), { name })}</p>}
        {confirmation && <TechnicalDetails entries={[[t("pluginUpdates.request"), <code key="id">{confirmation.requestId}</code>]]} />}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={!canWrite} onClick={(event) => {
            event.preventDefault(); if (confirmation) void execute(confirmation);
          }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}
