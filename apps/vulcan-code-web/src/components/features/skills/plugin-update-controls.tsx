import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { useI18n, type TranslationKey } from "@/i18n";
import type { LocalPluginUpdateStatus } from "@/service/plugin-update-service";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";

// PluginUpdateControlProps carries authoritative installed identities and the existing service action chain.
// PluginUpdateControlProps 携带权威已安装身份及既有服务动作链。
interface PluginUpdateControlProps {
  // plugins only suggests identities; retained requests remain queryable by an explicitly entered identifier.
  // plugins 仅建议身份；保留请求仍可通过明确输入标识查询。
  plugins: PluginSummary[];
  // actions preserve original request identity and apply the returned catalog through the ordinary publisher.
  // actions 保留原请求身份，并通过普通发布器应用返回目录。
  actions: Pick<SkillActionProps, "onReadLocalPluginUpdates" | "onResumeLocalPluginUpdate" | "onAbortLocalPluginUpdate">;
}

// phaseLabels presents durable stages without implying current readiness or successful external cleanup.
// phaseLabels 展示持久阶段，不暗示当前就绪或外部清理成功。
const phaseLabels: Record<LocalPluginUpdateStatus["phase"], TranslationKey> = {
  preparing: "pluginUpdates.preparing",
  prepared: "pluginUpdates.prepared", committing: "pluginUpdates.committing", published: "pluginUpdates.published",
  completed: "pluginUpdates.completed", aborted: "pluginUpdates.aborted",
};

// effectLabels distinguishes the exact recorded operation without implying automatic replay.
// effectLabels 区分精确记录操作，不暗示自动重放。
const effectLabels: Record<LocalPluginUpdateStatus["effects"][number]["operation"], TranslationKey> = {
  drain_predecessor_lua: "pluginUpdates.drainPredecessorLua",
  disable_predecessor: "pluginUpdates.disablePredecessor",
  probe: "pluginUpdates.probe", enable: "pluginUpdates.enable", enable_rollback: "pluginUpdates.enableRollback",
};

// PluginUpdateControls opens a read-only recovery surface and keeps its original state across collapse.
// PluginUpdateControls 打开只读恢复界面，并跨折叠保留原状态。
// Props supply existing service actions; returns controls without any automatic update or recovery.
// props 提供既有服务动作；返回控件，不自动更新或恢复。
export function PluginUpdateControls(props: PluginUpdateControlProps) {
  // Opening mounts once; hiding cannot discard an uncertain original request or in-flight acknowledgement.
  // 展开仅挂载一次；隐藏不能丢弃不确定原请求或进行中的确认。
  const [opened, setOpened] = React.useState(false);
  // Visibility does not own the lifetime of retained recovery state.
  // 可见性不拥有保留恢复状态的寿命。
  const [expanded, setExpanded] = React.useState(false);
  // Accessible panel identity stays stable across visibility changes.
  // 无障碍面板身份跨可见性变更保持稳定。
  const contentId = React.useId();
  // Text follows the currently selected application locale.
  // 文案跟随当前选定应用语言。
  const { t } = useI18n();
  return <section className="space-y-3 rounded-md border p-3">
    <Button type="button" variant="ghost" size="sm" aria-expanded={expanded} aria-controls={contentId}
      onClick={() => { setOpened(true); setExpanded(!expanded); }}>{t("pluginUpdates.title")}</Button>
    <div id={contentId} hidden={!expanded}>{opened && <PluginUpdateManagement {...props} />}</div>
  </section>;
}

// PluginUpdateManagement binds every observation and explicit operation to the exact selected plugin and original request.
// PluginUpdateManagement 将每次观测及明确操作绑定至精确选定插件和原请求。
// Props provide service ownership; returns an explicit-query panel with guarded recovery and abandonment actions.
// props 提供服务归属；返回带保护的恢复及放弃操作的明确查询面板。
function PluginUpdateManagement(props: PluginUpdateControlProps) {
  // selection and observation stay distinct so a late response cannot authorize another plugin's action.
  // 选择与观测保持分离，使迟到响应不能授权另一插件的操作。
  const [pluginId, setPluginId] = React.useState("");
  // The last actual read retains its explicit owner independently from the edited input.
  // 最后实际读取独立于编辑输入保留其明确所有者。
  const [observation, setObservation] = React.useState<{ pluginId: string; records: LocalPluginUpdateStatus[] } | null>(null);
  // Failed or invalidated reads cannot authorize mutations.
  // 失败或失效读取不能授权变更。
  const [fresh, setFresh] = React.useState(false);
  // Mutation and readback diagnostics remain visible until the next explicit action.
  // 变更与回读诊断保持可见，直至下一次明确操作。
  const [error, setError] = React.useState("");
  // Mutation ownership disables identity editing while reads permit invalidating their result.
  // 变更所有权停用身份编辑，而读取允许使其结果失效。
  const [busy, setBusy] = React.useState<"read" | "mutation" | null>(null);
  // The shared confirmation retains the exact reviewed request, never a list position.
  // 共享确认保留精确已审阅请求，绝不保留列表位置。
  const [confirmation, setConfirmation] = React.useState<LocalPluginUpdateStatus | null>(null);
  // Unconfirmed original operations survive empty rereads and owner changes until their exact record is observed.
  // 未确认原操作跨空回读及所有者变更存续，直至观察到其精确记录。
  const [unconfirmed, setUnconfirmed] = React.useState<LocalPluginUpdateStatus | null>(null);
  // Synchronous admission prevents repeat clicks before React commits the visual busy state.
  // 同步入场阻止 React 提交可见忙碌状态前的重复点击。
  const busyRef = React.useRef(false);
  // Observation version invalidates prior reads on explicit owner changes or component disposal.
  // 观测版本在明确所有者变更或组件销毁时使先前读取失效。
  const versionRef = React.useRef(0);
  // Disposal invalidates all asynchronous UI writes without cancelling service-owned work.
  // 销毁使全部异步界面写入失效，但不取消服务自有工作。
  const aliveRef = React.useRef(true);
  // One stable identifier associates the label, input and optional identity suggestions.
  // 单个稳定标识关联标签、输入及可选身份建议。
  const fieldId = React.useId();
  // The existing locale catalog owns all user-visible labels.
  // 既有语言目录持有全部用户可见标签。
  const { t } = useI18n();
  React.useEffect(() => {
    aliveRef.current = true;
    return () => { aliveRef.current = false; versionRef.current += 1; };
  }, []);

  // load reads one owner for one observation version; rejection remains visible to its original caller.
  // load 为单个观测版本读取单个所有者；拒绝继续对其原调用方可见。
  // owner/version are captured before awaiting; expectedRequest is required only when reconciling a known mutation.
  // owner/version 在等待前捕获；仅在核对已知变更时要求 expectedRequest。
  // Returns after a matching response; a missing original mutation record fails without replacing retained evidence.
  // 在匹配响应后返回；原变更记录缺失时直接失败，不替换保留证据。
  const load = async (owner: string, version: number, expectedRequest?: string): Promise<void> => {
    // records are already validated by the service adapter against owner.
    // records 已由服务适配器按 owner 校验。
    const records = await props.actions.onReadLocalPluginUpdates(owner);
    if (expectedRequest !== undefined && !records.some((record) => record.requestId === expectedRequest)) {
      throw new Error(t("pluginUpdates.missingOriginal"));
    }
    if (aliveRef.current && versionRef.current === version) {
      setObservation({ pluginId: owner, records }); setFresh(true);
      if (expectedRequest !== undefined) setUnconfirmed(null);
    }
  };

  // read explicitly queries the entered namespace and clears write eligibility until the exact response arrives.
  // read 明确查询输入命名空间，并清除写入资格，直至精确响应到达。
  // No inputs or return value; it never resumes or abandons any request.
  // 无输入或返回值；绝不恢复或放弃任何请求。
  const read = async (): Promise<void> => {
    if (busyRef.current || !pluginId.trim()) return;
    // Freeze exact query identity independently from later text edits.
    // 独立于后续文本编辑固定精确查询身份。
    const owner = pluginId.trim();
    // This increment binds the response to this exact explicit read.
    // 此递增将响应绑定至本次精确明确读取。
    const version = ++versionRef.current;
    busyRef.current = true; setBusy("read"); setFresh(false); setError(""); setConfirmation(null);
    // A retained uncertain operation requires its original record on every subsequent same-owner query.
    // 保留的不确定操作要求每次后续同所有者查询都包含其原记录。
    const expectedRequest = unconfirmed !== null && unconfirmed.pluginId === owner ? unconfirmed.requestId : undefined;
    try { await load(owner, version, expectedRequest); }
    catch (reason) { if (aliveRef.current && versionRef.current === version) setError(String(reason)); }
    finally { busyRef.current = false; if (aliveRef.current) setBusy(null); }
  };

  // execute runs one reviewed original action, then re-reads durable state even when the foreground response fails.
  // execute 执行单个已审阅原操作，即使前台响应失败也重新读取持久状态。
  // kind chooses the explicit operation and record fixes its identity; returns after the observation attempt.
  // kind 选择明确操作，record 固定其身份；在观测尝试之后返回。
  const execute = async (kind: "resume" | "abort", record: LocalPluginUpdateStatus): Promise<void> => {
    if (busyRef.current || unconfirmed !== null || !fresh || observation?.pluginId !== pluginId.trim() || record.pluginId !== observation.pluginId) return;
    // Operation identity cannot change while the service owns the mutation.
    // 服务拥有变更期间，操作身份不能改变。
    const version = ++versionRef.current;
    setUnconfirmed(record);
    busyRef.current = true; setBusy("mutation"); setFresh(false); setError("");
    // Keep mutation and readback failures independently; a successful read cannot erase the original error.
    // 独立保留变更与回读失败；成功读取不能抹去原错误。
    const failures: string[] = [];
    try {
      if (kind === "resume") await props.actions.onResumeLocalPluginUpdate(record.pluginId, record.requestId);
      else await props.actions.onAbortLocalPluginUpdate(record.pluginId, record.requestId, true);
    } catch (reason) { failures.push(String(reason)); }
    try { await load(record.pluginId, version, record.requestId); }
    catch (reason) { failures.push(String(reason)); }
    if (aliveRef.current && versionRef.current === version) {
      setError(failures.join("\n")); setConfirmation(null); setBusy(null);
    }
    busyRef.current = false;
  };

  // Only a matching successful observation enables explicit mutations; durable phase alone never proves live readiness.
  // 仅匹配成功观测启用明确变更；持久阶段本身绝不证明实时就绪。
  const canWrite = fresh && busy === null && unconfirmed === null && observation?.pluginId === pluginId.trim();
  return <div className="space-y-3 text-sm">
    <p className="text-muted-foreground">{t("pluginUpdates.description")}</p>
    <div className="flex flex-wrap items-end gap-2">
      <div className="min-w-48 flex-1 space-y-1">
        <label htmlFor={fieldId}>{t("pluginUpdates.plugin")}</label>
        <Input id={fieldId} value={pluginId} list={`${fieldId}-owners`} disabled={busy === "mutation"}
          onChange={(event) => { versionRef.current += 1; setPluginId(event.target.value); setFresh(false); setObservation(null); setError(""); setConfirmation(null); }} />
        <datalist id={`${fieldId}-owners`}>{props.plugins.filter((plugin) => plugin.extension?.origin === "userInstalled")
          .map((plugin) => <option key={plugin.id} value={plugin.id}>{plugin.name}</option>)}</datalist>
      </div>
      <Button type="button" size="sm" variant="outline" disabled={busy !== null || !pluginId.trim()}
        onClick={() => void read()}>{t("pluginUpdates.read")}</Button>
      {busy !== null && <span role="status">{t("ui.loading")}</span>}
    </div>
    {error && <p role="alert" className="whitespace-pre-wrap break-all text-destructive">{error}</p>}
    {unconfirmed && busy === null && <p role="status" className="break-all">{t("pluginUpdates.unconfirmed")}
      <br />{unconfirmed.pluginId} / <code>{unconfirmed.requestId}</code></p>}
    {!fresh && observation && <p role="status">{t("pluginUpdates.stale")}</p>}
    {fresh && observation?.records.length === 0 && <p role="status">{t("pluginUpdates.empty")}</p>}
    {observation?.records.map((record) => <article key={record.requestId} className="space-y-2 rounded-md border p-3">
      <p className="font-medium">{record.pluginId} · {record.candidate.version} · {t(phaseLabels[record.phase])}</p>
      <p className="break-all">{t("pluginUpdates.request")}: <code>{record.requestId}</code></p>
      <p>{t("pluginUpdates.installation")}: {record.installation === null ? t("pluginUpdates.noReceipt")
        : record.installation === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")}</p>
      {record.effects.map((effect) => <p key={effect.operation}>{t(effectLabels[effect.operation])}: {
        effect.phase === "started" ? t("pluginUpdates.effectStarted")
          : effect.phase === "succeeded" ? t("pluginUpdates.installSucceeded") : t("pluginUpdates.installUncertain")
      }</p>)}
      <div className="flex flex-wrap gap-2">
        {(record.phase === "committing" || record.phase === "published") && <Button type="button" size="sm" disabled={!canWrite}
          onClick={() => void execute("resume", record)}>{t("pluginUpdates.resume")}</Button>}
        {(record.phase === "preparing" || record.phase === "prepared" || record.phase === "committing") && <Button type="button" size="sm" variant="outline" disabled={!canWrite}
          onClick={() => setConfirmation(record)}>{t("pluginUpdates.abandon")}</Button>}
      </div>
    </article>)}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open && !busyRef.current) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("pluginUpdates.abandon")}</AlertDialogTitle>
          <AlertDialogDescription>{t("pluginUpdates.abandonWarning")}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation && <p className="break-all text-sm">{confirmation.pluginId} / {confirmation.requestId}</p>}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy !== null}>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={!canWrite} onClick={(event) => {
            event.preventDefault(); if (confirmation) void execute("abort", confirmation);
          }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
