import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { useI18n, type TranslationKey } from "@/i18n";
import type { PluginInstallPhase, PluginInstallStatus } from "@/service/plugin-install-recovery-service";
import type { PluginSecretMutationPhase, PluginSecretMutationStatus } from "@/service/plugin-secret-recovery-service";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// RecoveryRecord is a non-secret observation of one actual retained service request.
// RecoveryRecord 是单个实际保留服务请求的无秘密观测。
type RecoveryRecord = PluginInstallStatus | PluginSecretMutationStatus;

// RecoveryProps supplies exact service actions and the same synchronous admission as ordinary plugin mutations.
// RecoveryProps 提供精确服务动作及与普通插件变更相同的同步入场。
interface RecoveryProps {
  // kind selects one declared service protocol; it never probes alternative request registries.
  // kind 选择一个声明服务协议；绝不探测替代请求注册表。
  kind: "installation" | "credential";
  // actions forward original plugin and request identifiers without reconstructing physical ownership.
  // actions 原样传递原插件及请求标识，不重建物理所有权。
  actions: Pick<SkillActionProps, "onReadPluginInstallRequests" | "onRecoverPluginInstallRequest" | "onReadPluginSecretMutations" | "onRecoverPluginSecretMutation">;
  // plugins and suggestedPluginId only suggest identifiers, including an inspected unpublished plugin.
  // plugins 及 suggestedPluginId 仅建议标识，包括已检查且尚未发布的插件。
  plugins: PluginSummary[];
  // The inspected source supplies this identifier; subsequent source edits do not replace a recovery observation.
  // 已检查来源提供此标识；随后来源编辑不替换恢复观测。
  suggestedPluginId: string;
  // mutationBusy reflects the parent page's shared lifecycle admission.
  // mutationBusy 反映父页面共享生命周期入场。
  mutationBusy: boolean;
  // acquireMutation returns true only after atomically acquiring the existing page mutation gate.
  // acquireMutation 仅在原子取得既有页面变更门后返回真。
  acquireMutation: () => boolean;
  // releaseMutation releases this same admission after the explicit recovery and readback attempt.
  // releaseMutation 在明确恢复及回读尝试后释放此相同入场。
  releaseMutation: () => void;
}

// phaseLabels deliberately communicates only facts proved by these two sanitized service protocols.
// phaseLabels 刻意仅传达这两个脱敏服务协议证明的事实。
const phaseLabels: Record<PluginInstallPhase | PluginSecretMutationPhase, TranslationKey> = {
  executing: "pluginRecovery.executing",
  reserved: "pluginRecovery.pending", packagePublication: "pluginRecovery.pending",
  credentialMutation: "pluginRecovery.pending", nativeValidation: "pluginRecovery.pending",
  lifecycle: "pluginRecovery.pending", publishing: "pluginRecovery.pending",
  aborting: "pluginRecovery.pending", nativeRefresh: "pluginRecovery.pending",
  aborted: "pluginRecovery.restored", restored: "pluginRecovery.restored",
  committed: "pluginRecovery.committed",
};

// PluginRequestRecoveryControls mounts an inert recovery panel once and retains its state across collapse.
// PluginRequestRecoveryControls 一次挂载静止恢复面板，并跨折叠保留其状态。
// props selects the exact protocol and admission; returns explicit query and recovery controls with no automatic mutation.
// props 选择精确协议及入场；返回明确查询及恢复控件，不自动变更。
export function PluginRequestRecoveryControls(props: RecoveryProps) {
  // opened keeps uncertain observations mounted after the first user expansion.
  // opened 在首次用户展开后保持不确定观测挂载。
  const [opened, setOpened] = React.useState(false);
  // expanded changes visibility without owning the lifetime of original service resources.
  // expanded 改变可见性，不拥有原服务资源寿命。
  const [expanded, setExpanded] = React.useState(false);
  // panelId keeps the disclosure associated with its stable accessible panel.
  // panelId 使展开控件与稳定无障碍面板关联。
  const panelId = React.useId();
  // t follows the current application locale.
  // t 跟随当前应用语言。
  const { t } = useI18n();
  return <section className="space-y-3 rounded-md border p-3">
    <Button type="button" variant="ghost" size="sm" aria-expanded={expanded} aria-controls={panelId}
      onClick={() => { setOpened(true); setExpanded(!expanded); }}>
      {t(props.kind === "installation" ? "pluginRecovery.installTitle" : "pluginRecovery.secretTitle")}
    </Button>
    <div id={panelId} hidden={!expanded}>{opened && <RecoveryManagement {...props} />}</div>
  </section>;
}

// RecoveryManagement binds every query, confirmation and mutation to the actual original UUID and namespace.
// RecoveryManagement 将每次查询、确认及变更绑定至实际原 UUID 及命名空间。
// props supplies one protocol; returns a recovery surface that never interprets missing ownership as successful cleanup.
// props 提供一个协议；返回绝不把所有权缺失解释为清理成功的恢复界面。
function RecoveryManagement(props: RecoveryProps) {
  // owner is the explicitly selected namespace, independent from installed catalog membership.
  // owner 是明确选定命名空间，独立于已安装目录成员关系。
  const [owner, setOwner] = React.useState(props.suggestedPluginId);
  // observation retains the actual query owner separately from editable text.
  // observation 独立于可编辑文本保留实际查询所有者。
  const [observation, setObservation] = React.useState<{ owner: string; records: RecoveryRecord[] } | null>(null);
  // fresh permits a mutation only after a successful matching explicit read.
  // fresh 仅在成功匹配明确读取之后允许变更。
  const [fresh, setFresh] = React.useState(false);
  // busy closes local admission for both querying and recovery.
  // busy 关闭查询及恢复的局部入场。
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // error retains recovery and subsequent readback failures independently from successful observations.
  // error 独立于成功观测保留恢复及后续回读失败。
  const [error, setError] = React.useState("");
  // confirmation freezes the reviewed original without retaining any credential value.
  // confirmation 固定已审阅原实例，不保留任何凭据值。
  const [confirmation, setConfirmation] = React.useState<RecoveryRecord | null>(null);
  // acknowledgement defaults to false and applies solely to the installation protocol's failed compensator.
  // acknowledgement 默认为假，仅适用于安装协议的失败补偿。
  const [acknowledgement, setAcknowledgement] = React.useState(false);
  // uncertain retains an unacknowledged foreground outcome across owner edits, collapse and empty rereads.
  // uncertain 跨所有者编辑、折叠及空回读保留未经确认的前台结果。
  const [uncertain, setUncertain] = React.useState<RecoveryRecord | null>(null);
  // confirmed records an actual successful recovery response even if later readback fails.
  // confirmed 记录实际成功恢复响应，即使后续回读失败。
  const [confirmed, setConfirmed] = React.useState<RecoveryRecord | null>(null);
  // busyRef prevents repeat clicks before React commits the busy state.
  // busyRef 阻止 React 提交忙碌状态之前的重复点击。
  const busyRef = React.useRef(false);
  // versionRef invalidates late query responses on identity changes or disposal.
  // versionRef 在身份变化或销毁时使迟到查询响应失效。
  const versionRef = React.useRef(0);
  // aliveRef protects renderer state without cancelling service-owned recovery.
  // aliveRef 保护渲染器状态，不取消服务自有恢复。
  const aliveRef = React.useRef(true);
  // fieldId associates the explicit namespace label and optional suggestions.
  // fieldId 关联明确命名空间标签及可选建议。
  const fieldId = React.useId();
  // t resolves existing locale-owned user copy.
  // t 解析既有语言目录持有的用户文案。
  const { t } = useI18n();
  React.useEffect(() => {
    aliveRef.current = true;
    return () => { aliveRef.current = false; versionRef.current += 1; };
  }, []);

  // load observes queryOwner for version; requiredOriginal prevents an uncertain disappearance from authorizing another action.
  // load 为 version 观测 queryOwner；requiredOriginal 阻止不确定消失授权另一动作。
  // Returns matching records, or rejects mismatched identities and missing uncertain originals without replacing evidence.
  // 返回匹配记录，或拒绝身份错配及不确定原实例缺失，不替换证据。
  const load = async (queryOwner: string, version: number, requiredOriginal: RecoveryRecord | null): Promise<void> => {
    // records comes exclusively from the declared service registry.
    // records 仅来自声明服务注册表。
    const records = props.kind === "installation"
      ? await props.actions.onReadPluginInstallRequests(queryOwner)
      : await props.actions.onReadPluginSecretMutations(queryOwner);
    if (records.some((record) => record.pluginId !== queryOwner)
      || new Set(records.map((record) => record.requestId)).size !== records.length) {
      throw new Error(t("pluginRecovery.identityMismatch"));
    }
    if (requiredOriginal !== null && !records.some((record) => record.requestId === requiredOriginal.requestId)) {
      throw new Error(t("pluginRecovery.unknown"));
    }
    if (aliveRef.current && versionRef.current === version) {
      setObservation({ owner: queryOwner, records }); setFresh(true);
      if (requiredOriginal !== null) setUncertain(null);
    }
  };

  // read explicitly observes the entered owner and never starts or retries recovery.
  // read 明确观测输入所有者，绝不启动或重试恢复。
  // No parameters; returns after matching observation or a visible query error.
  // 无参数；在匹配观测或可见查询错误后返回。
  const read = async (): Promise<void> => {
    if (busyRef.current || !owner.trim()) return;
    // queryOwner freezes the namespace independently from text edits during the await.
    // queryOwner 独立于等待期间文本编辑固定命名空间。
    const queryOwner = owner.trim();
    // version identifies this explicit observation only.
    // version 仅标识本次明确观测。
    const version = ++versionRef.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null);
    try { await load(queryOwner, version, uncertain?.pluginId === queryOwner ? uncertain : null); }
    catch (reason) { if (aliveRef.current && versionRef.current === version) setError(String(reason)); }
    finally { busyRef.current = false; if (aliveRef.current) setBusy(null); }
  };

  // execute borrows the page gate, recovers the reviewed UUID once, then observes the same owner on every outcome.
  // execute 借用页面门，一次恢复已审阅 UUID，然后在每种结果下观测相同所有者。
  // record is the exact reviewed non-secret original; returns without claiming success from an empty registry.
  // record 是精确已审阅无秘密原实例；返回时不以空注册表声称成功。
  const execute = async (record: RecoveryRecord): Promise<void> => {
    if (busyRef.current || !fresh || uncertain !== null || props.mutationBusy || record.phase === "executing"
      || observation?.owner !== owner.trim() || record.pluginId !== observation.owner || !props.acquireMutation()) return;
    // version binds the response and readback to the selected original.
    // version 将响应及回读绑定至选定原实例。
    const version = ++versionRef.current;
    // failures retains the foreground failure even if the subsequent read succeeds.
    // failures 即使后续读取成功也保留前台失败。
    const failures: string[] = [];
    // succeeded is true only after the actual recovery RPC resolves successfully.
    // succeeded 仅在实际恢复 RPC 成功结束后为真。
    let succeeded = false;
    busyRef.current = true; setBusy("mutation"); setFresh(false); setUncertain(record); setConfirmed(null); setError("");
    try {
      try {
        if (props.kind === "installation") await props.actions.onRecoverPluginInstallRequest(record.pluginId, record.requestId, acknowledgement);
        else await props.actions.onRecoverPluginSecretMutation(record.pluginId, record.requestId);
        succeeded = true;
        if (aliveRef.current && versionRef.current === version) { setConfirmed(record); setUncertain(null); }
      } catch (reason) { failures.push(String(reason)); }
      try { await load(record.pluginId, version, succeeded ? null : record); }
      catch (reason) { failures.push(String(reason)); }
      if (aliveRef.current && versionRef.current === version) { setError(failures.join("\n")); setConfirmation(null); }
    } finally {
      props.releaseMutation(); busyRef.current = false;
      if (aliveRef.current) setBusy(null);
    }
  };

  // canWrite requires a fresh matching namespace and no unresolved original operation anywhere in this panel.
  // canWrite 要求新鲜匹配命名空间，且此面板任何位置均无未解决原操作。
  const canWrite = fresh && !busy && !props.mutationBusy && uncertain === null && observation?.owner === owner.trim();
  return <div className="space-y-3 text-sm">
    <p className="text-muted-foreground">{t(props.kind === "installation" ? "pluginRecovery.installDescription" : "pluginRecovery.secretDescription")}</p>
    <div className="flex flex-wrap items-end gap-2">
      <div className="min-w-48 flex-1 space-y-1">
        <label htmlFor={fieldId}>{t("pluginUpdates.plugin")}</label>
        <Input id={fieldId} value={owner} list={`${fieldId}-owners`} disabled={props.mutationBusy || busy === "mutation"}
          onChange={(event) => { versionRef.current += 1; setOwner(event.target.value); setObservation(null); setFresh(false); setConfirmation(null); setError(""); }} />
        <datalist id={`${fieldId}-owners`}>
          {props.suggestedPluginId && <option value={props.suggestedPluginId} />}
          {props.plugins.filter((plugin) => plugin.id !== props.suggestedPluginId).map((plugin) => <option key={plugin.id} value={plugin.id}>{plugin.name}</option>)}
        </datalist>
      </div>
      <Button type="button" size="sm" variant="outline" disabled={busy !== null || !owner.trim()} onClick={() => void read()}>{t("pluginRecovery.read")}</Button>
    </div>
    {uncertain && <p role="status" className="break-all">{t("pluginRecovery.unknown")} {uncertain.pluginId} / {uncertain.requestId}</p>}
    {confirmed && <p role="status" className="break-all">{t("pluginRecovery.confirmed")} {confirmed.pluginId} / {confirmed.requestId}</p>}
    {error && <p role="alert" className="whitespace-pre-wrap">{error}</p>}
    {observation && !fresh && <p>{t("pluginUpdates.stale")}</p>}
    {observation && fresh && observation.records.length === 0 && <p>{t("pluginRecovery.empty")}</p>}
    {observation?.records.map((record) => <article key={record.requestId} className="space-y-2 rounded-md border p-3">
      <p className="break-all">{record.pluginId} / {t("pluginUpdates.request")}: {record.requestId}</p>
      {"serverId" in record && <p className="break-all">{record.serverId} / {record.secretKey}</p>}
      <p>{t(phaseLabels[record.phase])}</p>
      <Button type="button" size="sm" disabled={!canWrite || record.phase === "executing"}
        onClick={() => { setConfirmation(record); setAcknowledgement(false); }}>{t("pluginRecovery.recover")}</Button>
    </article>)}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("pluginRecovery.recover")}</AlertDialogTitle>
          <AlertDialogDescription>{t(props.kind === "installation" ? "pluginRecovery.installWarning" : "pluginRecovery.secretWarning")}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation && <p className="break-all text-sm">{confirmation.pluginId} / {confirmation.requestId}</p>}
        {props.kind === "installation" && confirmation?.phase !== "committed" && <label className="flex items-start gap-2 text-sm">
          <Checkbox checked={acknowledgement} disabled={busy !== null} onCheckedChange={(checked) => setAcknowledgement(checked === true)} />
          {t("pluginRecovery.acknowledgement")}
        </label>}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={!canWrite} onClick={(event) => {
            event.preventDefault(); if (confirmation) void execute(confirmation);
          }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
