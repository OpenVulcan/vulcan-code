import * as React from "react";
import { PackageMinus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { formatMessage, useI18n } from "@/i18n";
import type { PluginUninstallStatus } from "@/service/plugin-uninstall-recovery-service";
import type { SkillActionProps } from "./skill-area-types";
import { MaintenanceItem, MaintenanceNotice, TechnicalDetails, useRecoveryReport, type RecoveryReport } from "./plugin-maintenance-section";

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

// Props bind all-request observation and exact continuation to the existing page mutation gate.
// Props 将全量请求观测及精确继续绑定至既有页面变更门。
export interface PluginUninstallRecoveryProps {
  // actions expose only the declared uninstall protocols, with no protocol probing.
  // actions 仅暴露声明卸载协议，不探测协议。
  actions: Pick<SkillActionProps, "onReadPluginUninstallRequests" | "onResumePluginUninstallRequest">;
  // initialUncertainty captures an unconfirmed ordinary uninstall response.
  // initialUncertainty 捕获未确认普通卸载响应。
  initialUncertainty: PluginUninstallUncertainty | null;
  // onConfirmed releases the foreground uncertainty only after actual continuation success.
  // onConfirmed 仅在实际继续成功后释放前台不确定状态。
  onConfirmed: (pluginId: string) => void;
  // mutationBusy reports all ordinary page mutations sharing this gate.
  // mutationBusy 报告共享此门的所有普通页面变更。
  mutationBusy: boolean;
  // acquireMutation atomically acquires the existing page admission and returns success.
  // acquireMutation 原子取得既有页面入场，并返回是否成功。
  acquireMutation: () => boolean;
  // releaseMutation releases only renderer admission, never service-owned originals.
  // releaseMutation 仅释放渲染器入场，绝不释放服务自有原实例。
  releaseMutation: () => void;
  // onLocateDiagnostics selects the original plugin/request pair for readonly blockers.
  // onLocateDiagnostics 为只读阻塞者选择原插件与请求组合。
  onLocateDiagnostics?: (pluginId: string, requestId: string) => void;
  // pluginId narrows the displayed originals to one plugin's drawer; the full read and its checks are unchanged.
  // pluginId 将显示的原实例收窄到单个插件抽屉；全量读取及其校验不变。
  pluginId?: string;
  // onRecordsObserved receives every successful full read so the page can surface retained originals.
  // onRecordsObserved 接收每次成功全量读取，使页面可提示保留的原实例。
  onRecordsObserved?: (records: PluginUninstallStatus[]) => void;
  // pluginName resolves a display name; uninstalled plugins may be absent from the catalog and fall back to their ID.
  // pluginName 解析显示名称；已卸载插件可能不在目录中，此时回退为其 ID。
  pluginName: (pluginId: string) => string;
  // scanToken changes whenever the container asks every reader to check again; the mount read is the first scan.
  // scanToken 在容器要求所有读取器重新检查时变化；挂载读取即首次扫描。
  scanToken?: number;
  // onReport tells the container whether anything is shown.
  // onReport 告知容器是否有显示内容。
  onReport?: (report: RecoveryReport) => void;
}

// Observe all retained uninstalls on mount so hidden plugins remain discoverable after a full page refresh.
// 挂载时观测全部保留卸载，使隐藏插件在整页刷新后仍可发现。
// props supplies the declared protocol and gate; returns plain-language rows with explicit continuation.
// props 提供声明协议及门；返回带明确继续操作的平实行。
export function PluginUninstallRecoveryControls(props: PluginUninstallRecoveryProps) {
  // records is the last successful full read, independent from the published plugin catalog.
  // records 是最近成功全量读取，独立于已发布插件目录。
  const [records, setRecords] = React.useState<PluginUninstallStatus[]>([]);
  // fresh permits continuation only after a successful matching observation.
  // fresh 仅在成功匹配观测后允许继续。
  const [fresh, setFresh] = React.useState(false);
  // busy and busyRef close asynchronous and pre-render repeat-click admission.
  // busy 及 busyRef 关闭异步与渲染前重复点击入场。
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // busyRef prevents simultaneous reads or repeated continuation dispatches.
  // busyRef 阻止同时读取或重复派发继续。
  const busyRef = React.useRef(false);
  // uncertain survives missing originals and must never be inferred complete from an empty read.
  // uncertain 在原实例缺失时保留，绝不从空读取推断完成。
  const [uncertain, setUncertain] = React.useState<PluginUninstallUncertainty | null>(props.initialUncertainty);
  // confirmed records an actual successful continuation despite any later readback failure.
  // confirmed 记录实际成功继续，即使后续回读失败。
  const [confirmed, setConfirmed] = React.useState<PluginUninstallStatus | null>(null);
  // confirmation freezes the explicitly reviewed original and service acknowledgement requirement.
  // confirmation 固定明确审阅原实例及服务确认要求。
  const [confirmation, setConfirmation] = React.useState<PluginUninstallStatus | null>(null);
  // acknowledgement starts false for each reviewed original and is never inferred from its phase.
  // acknowledgement 对每个审阅原实例初始为假，绝不从阶段推断。
  const [acknowledgement, setAcknowledgement] = React.useState(false);
  // error preserves foreground and observation failures without hiding uncertain ownership.
  // error 保留前台及观测失败，不隐藏不确定所有权。
  const [error, setError] = React.useState("");
  // version and alive suppress late renderer writes without cancelling actual service work.
  // version 及 alive 抑制迟到渲染器写入，不取消实际服务工作。
  const version = React.useRef(0);
  // alive guards component state after disposal.
  // alive 在销毁后保护组件状态。
  const alive = React.useRef(true);
  // t resolves locale-owned user copy.
  // t 解析语言目录持有的用户文案。
  const { t } = useI18n();

  React.useEffect(() => {
    if (props.initialUncertainty !== null) {
      version.current += 1; setUncertain(props.initialUncertainty); setFresh(false); setConfirmation(null);
    }
  }, [props.initialUncertainty]);

  // load reads every original and requires any uncertain UUID to remain physically queryable.
  // load 读取所有原实例，并要求任何不确定 UUID 仍可物理查询。
  // expected retains failed response identity; token rejects stale reads; returns without treating absence as success.
  // expected 保留失败响应身份；token 拒绝过期读取；返回时不将缺失当作成功。
  const load = async (expected: PluginUninstallUncertainty | null, token: number): Promise<void> => {
    // observed comes exclusively from the declared all-request service adapter.
    // observed 仅来自声明全量请求服务适配器。
    const observed = await props.actions.onReadPluginUninstallRequests();
    if (new Set(observed.map((record) => record.requestId)).size !== observed.length) throw new Error(t("pluginRecovery.identityMismatch"));
    if (expected !== null) {
      // matching selects the actual original UUID or the sole observed namespace when no UUID was captured.
      // matching 选择实际原 UUID，未捕获 UUID 时选择唯一观测命名空间。
      const matching = observed.filter((record) => record.pluginId === expected.pluginId
        && (expected.requestId === null || record.requestId === expected.requestId));
      if (matching.length === 0) throw new Error(t("pluginRecovery.uninstallUnknown"));
      if (matching.length !== 1) throw new Error(t("pluginRecovery.identityMismatch"));
    }
    if (alive.current && version.current === token) {
      setRecords(observed); setFresh(true);
      if (expected !== null) setUncertain(null);
      props.onRecordsObserved?.(observed);
    }
  };

  // read performs only an explicit full observation and never retries an uninstall.
  // read 仅执行明确全量观测，绝不重试卸载。
  // No parameters; returns after the original status or a visible error is observed.
  // 无参数；观测原状态或可见错误后返回。
  const read = async (): Promise<void> => {
    if (busyRef.current) return;
    // token identifies this observation without changing the original request identity.
    // token 标识此观测，不改变原请求身份。
    const token = ++version.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null); setConfirmed(null);
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

  React.useEffect(() => {
    alive.current = true;
    // disposed belongs to this mount effect, including React's development remount probe.
    // disposed 属于此挂载效果，包括 React 的开发重挂载探测。
    let disposed = false;
    // token prevents the discarded mount's response from overwriting the active mount.
    // token 阻止已丢弃挂载响应覆盖活动挂载。
    const token = ++version.current;
    busyRef.current = true; setBusy("read"); setFresh(false);
    // Mount observes the registry even while collapsed; it dispatches no continuation.
    // 挂载即使折叠也观测注册表；不派发继续。
    void load(props.initialUncertainty, token).catch((reason: unknown) => {
      if (!disposed && alive.current && version.current === token) setError(String(reason));
    }).finally(() => { if (!disposed) { busyRef.current = false; if (alive.current) setBusy(null); } });
    return () => { disposed = true; alive.current = false; version.current += 1; busyRef.current = false; };
    // This first read is tied to the mounted service surface, not parent render frequency.
    // 首次读取绑定已挂载服务界面，而非父级渲染频率。
  }, []);

  // execute continues exactly one reviewed UUID after obtaining the shared synchronous page admission.
  // execute 取得共享同步页面入场后，精确继续单个审阅 UUID。
  // record is the actual observed original; returns after RPC and readback, retaining uncertain failures.
  // record 是实际观测原实例；RPC 及回读后返回，保留不确定失败。
  const execute = async (record: PluginUninstallStatus): Promise<void> => {
    if (busyRef.current || !fresh || uncertain !== null || props.mutationBusy || record.phase === "executing"
      || (record.requiresExternalEffectsAcknowledgement && !acknowledgement)
      || !records.includes(record) || !props.acquireMutation()) return;
    // token binds all late renderer writes to this explicit continuation.
    // token 将所有迟到渲染器写入绑定至此明确继续。
    const token = ++version.current;
    // failures retain the foreground error even when reading the original succeeds.
    // failures 即使读取原实例成功也保留前台错误。
    const failures: string[] = [];
    // succeeded derives only from the actual RPC success, never registry absence.
    // succeeded 仅源于实际 RPC 成功，绝不源于注册表缺失。
    let succeeded = false;
    busyRef.current = true; setBusy("mutation"); setFresh(false); setUncertain(record); setConfirmed(null); setError("");
    try {
      try {
        await props.actions.onResumePluginUninstallRequest(record.pluginId, record.requestId,
          record.requiresExternalEffectsAcknowledgement && acknowledgement);
        succeeded = true;
        props.onConfirmed(record.pluginId);
        if (alive.current && version.current === token) { setConfirmed(record); setUncertain(null); }
      } catch (reason) { failures.push(String(reason)); }
      try { await load(succeeded ? null : record, token); }
      catch (reason) { failures.push(String(reason)); }
      if (alive.current && version.current === token) { setError(failures.join("\n")); setConfirmation(null); }
    } finally {
      props.releaseMutation();
      if (version.current === token) { busyRef.current = false; setBusy(null); }
    }
  };

  // canWrite requires fresh evidence and excludes unresolved foreground outcomes.
  // canWrite 要求新鲜证据，并排除未解决前台结果。
  const canWrite = fresh && busy === null && !props.mutationBusy && uncertain === null;
  // visibleRecords keeps the exact observed objects so continuation still matches them by identity.
  // visibleRecords 保留精确观测对象，使继续操作仍按身份匹配。
  const visibleRecords = props.pluginId === undefined ? records : records.filter((record) => record.pluginId === props.pluginId);
  const showUncertain = uncertain !== null && busy === null;
  const visible = visibleRecords.length + (showUncertain ? 1 : 0) + (confirmed ? 1 : 0) + (error ? 1 : 0);
  useRecoveryReport(props.onReport, !fresh && !error && records.length === 0, visible);
  return <>
    {showUncertain && <MaintenanceNotice tone="warning">{formatMessage(t("pluginRecovery.uninstallUnknownNotice"), { name: props.pluginName(uncertain.pluginId) })}</MaintenanceNotice>}
    {confirmed && <MaintenanceNotice tone="success">{formatMessage(t("pluginRecovery.uninstallConfirmed"), { name: props.pluginName(confirmed.pluginId) })}</MaintenanceNotice>}
    {error && <MaintenanceNotice tone="error"><span className="whitespace-pre-wrap break-all">{t("pluginRecovery.uninstallFailed")}{"\n"}{error}</span></MaintenanceNotice>}
    {visibleRecords.map((record) => {
      const title = formatMessage(t("pluginRecovery.uninstallItemTitle"), { name: props.pluginName(record.pluginId) });
      const executing = record.phase === "executing";
      return <MaintenanceItem
        actions={<>
          {props.onLocateDiagnostics !== undefined && <Button onClick={() => props.onLocateDiagnostics?.(record.pluginId, record.requestId)} size="sm" type="button" variant="ghost">
            {t("pluginDiagnostics.locateRequest")}</Button>}
          {!executing && <Button disabled={!canWrite} onClick={() => { setConfirmation(record); setAcknowledgement(false); }} size="sm" type="button">
            {t("pluginRecovery.uninstallContinue")}</Button>}
        </>}
        description={t(executing ? "pluginRecovery.hint.executing" : "pluginRecovery.hint.uninstall")}
        icon={<PackageMinus size={16} />}
        key={record.requestId}
        label={title}
        title={<>{title}<span className="plugin-maintenance-item-status">{t(executing ? "pluginRecovery.executing" : "pluginRecovery.pending")}</span></>}
        tone={executing ? "neutral" : "attention"}
      >
        <TechnicalDetails entries={[
          [t("pluginUpdates.plugin"), <code key="plugin">{record.pluginId}</code>],
          [t("pluginUpdates.request"), <code key="id">{record.requestId}</code>],
          [t("pluginRecovery.phase"), <code key="phase">{record.phase}</code>],
        ]} />
      </MaintenanceItem>;
    })}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader><AlertDialogTitle>{t("pluginRecovery.uninstallContinue")}</AlertDialogTitle>
          <AlertDialogDescription>{t("pluginRecovery.uninstallWarning")}</AlertDialogDescription></AlertDialogHeader>
        {confirmation && <p className="text-sm">{formatMessage(t("pluginRecovery.uninstallItemTitle"), { name: props.pluginName(confirmation.pluginId) })}</p>}
        {confirmation && <TechnicalDetails entries={[[t("pluginUpdates.request"), <code key="id">{confirmation.requestId}</code>]]} />}
        {confirmation?.requiresExternalEffectsAcknowledgement === true && <label className="flex items-start gap-2 text-sm">
          <Checkbox checked={acknowledgement} disabled={busy !== null} onCheckedChange={(checked) => setAcknowledgement(checked === true)} />
          {t("pluginRecovery.uninstallAcknowledgement")}
        </label>}
        <AlertDialogFooter><AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={!canWrite || (confirmation?.requiresExternalEffectsAcknowledgement === true && !acknowledgement)}
            onClick={(event) => { event.preventDefault(); if (confirmation) void execute(confirmation); }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </>;
}
