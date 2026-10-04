import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary } from "@/types";
import { PLUGIN_CAPACITY_PURPOSES, type PluginCapacityPurpose } from "@/types/plugin-runtime";
import type { PluginRuntimeCommand } from "@/service/plugin-runtime-service";
import { capacityPredecessor, createCapacityDraft, draftStillCurrent, readCapacityDraft, type PluginCapacityDraft } from "./plugin-capacity-draft";
import { usePluginRuntimeManagement } from "./use-plugin-runtime-management";

// PluginRuntimeControlProps supplies installed ownership and the existing explicit business-catalog refresh.
// PluginRuntimeControlProps 提供已安装归属及现有明确业务目录刷新。
interface PluginRuntimeControlProps {
  // plugins is the authoritative installed summary; retained capacities may outlive these entries.
  // plugins 为权威已安装摘要；保留容量的寿命可能超出这些条目。
  plugins: PluginSummary[];
  // onRefreshCatalog republishes business availability after the user has completed policy repair.
  // onRefreshCatalog 在用户完成策略修复后重新发布业务可用性。
  onRefreshCatalog: () => Promise<void>;
}

// numericLabels maps each native editable numeric field to localized copy without introducing budget defaults.
// numericLabels 将每个原生可编辑数字字段映射到本地化文案，不引入预算默认值。
const numericLabels: Record<keyof PluginCapacityDraft["numbers"], TranslationKey> = {
  min_resident_vms: "pluginRuntime.min", max_resident_vms: "pluginRuntime.max",
  max_running_calls: "pluginRuntime.running", max_queued_calls: "pluginRuntime.queueCalls", max_queued_bytes: "pluginRuntime.queueBytes",
};

// PluginRuntimeControls renders a collapsed host-owned entry; opening it only reads existing runtime state.
// PluginRuntimeControls 渲染折叠的宿主入口；展开仅读取现有运行时状态。
export function PluginRuntimeControls(props: PluginRuntimeControlProps) {
  // opened retains local uncertain requests when the user temporarily collapses this panel.
  // opened 在用户暂时折叠面板时保留本地不确定请求。
  const [opened, setOpened] = React.useState(false);
  const [expanded, setExpanded] = React.useState(false);
  const { t } = useI18n();
  const contentId = React.useId();
  return <section className="space-y-3 rounded-md border p-3">
    <Button type="button" variant="ghost" size="sm" aria-expanded={expanded} aria-controls={contentId}
      onClick={() => { setOpened(true); setExpanded(!expanded); }}>{t("pluginRuntime.title")}</Button>
    <div id={contentId} hidden={!expanded}>{opened && <PluginRuntimeManagement {...props} />}</div>
  </section>;
}

// PluginRuntimeManagement renders explicit, predecessor-fenced commands; props never grant plugin code a new API.
// PluginRuntimeManagement 渲染明确且受前驱约束的操作；props 不给插件代码新增接口。
function PluginRuntimeManagement(props: PluginRuntimeControlProps) {
  // state owns observations; draft and confirmation preserve the identities that the user reviewed.
  // state 持有观测；draft 和 confirmation 保留用户审阅的身份。
  const state = usePluginRuntimeManagement();
  const { t } = useI18n();
  const [pluginId, setPluginId] = React.useState("");
  const [purpose, setPurpose] = React.useState<PluginCapacityPurpose>("tool_call");
  const [draft, setDraft] = React.useState<PluginCapacityDraft | null>(null);
  const [formError, setFormError] = React.useState("");
  const [confirmation, setConfirmation] = React.useState<{ command: PluginRuntimeCommand | null; warning: TranslationKey } | null>(null);
  const [catalogBusy, setCatalogBusy] = React.useState(false);
  const catalogBusyRef = React.useRef(false);
  const fieldPrefix = React.useId();
  // Retained owners remain selectable after uninstall, and the global receipt stays visible across selection changes.
  // 卸载后保留所有者仍可选择；全局回执跨选择变化保持可见。
  const catalog = state.observation?.catalog;
  const runtime = state.observation?.runtime;
  const receipt = runtime?.policyUpdate;
  const pluginIds = [...new Set([
    ...props.plugins.filter((plugin) => plugin.extension?.hasLuaRuntime).map((plugin) => plugin.id),
    ...(catalog?.capacities.map((capacity) => capacity.snapshot.plugin_id) ?? []),
  ])].sort();
  const selected = catalog?.capacities.find((capacity) => capacity.snapshot.plugin_id === pluginId && capacity.purpose === purpose);
  const draftOwner = draft ? catalog?.capacities.find((capacity) => capacity.snapshot.plugin_id === draft.predecessor.pluginId && capacity.purpose === draft.predecessor.purpose) : undefined;
  const draftCurrent = draft !== null && draftStillCurrent(draft, draftOwner);
  const canWrite = state.fresh && !state.busy && !state.pending && !catalogBusy;
  const canEdit = canWrite && !receipt && selected && !selected.snapshot.closing && !selected.retirementRequested;
  const canRegister = canWrite && catalog && !selected && props.plugins.some((plugin) => plugin.id === pluginId && plugin.extension?.hasLuaRuntime);

  // submit validates a full draft before allocating a transaction UUID; returns nothing.
  // submit 在分配事务 UUID 前校验完整草稿；无返回值。
  const submit = (): void => {
    if (!draft || !draftCurrent || !canWrite || receipt) return;
    try {
      // config contains every current native field, with no inherited values guessed by the browser.
      // config 包含全部当前原生字段，浏览器不猜测继承值。
      const config = readCapacityDraft(draft);
      setFormError("");
      void state.execute({ kind: "update", request: { ...draft.predecessor, updateId: crypto.randomUUID(), config } });
    } catch (reason) { setFormError(String(reason)); }
  };

  // refreshCatalog invokes the existing explicit publication action and reports failures without altering policy.
  // refreshCatalog 调用现有明确发布操作并报告失败，不修改策略。
  const refreshCatalog = async (): Promise<void> => {
    if (catalogBusyRef.current) return;
    catalogBusyRef.current = true; setCatalogBusy(true); setFormError("");
    try { await props.onRefreshCatalog(); await state.refresh(); }
    catch (reason) { setFormError(String(reason)); }
    finally { catalogBusyRef.current = false; setCatalogBusy(false); }
  };

  return <div className="space-y-3 text-sm">
    <p className="text-muted-foreground">{t("pluginRuntime.description")}</p>
    <div className="flex flex-wrap items-center gap-2">
      <Button type="button" size="sm" variant="outline" disabled={state.busy || catalogBusy} onClick={() => void state.refresh()}>{t("pluginRuntime.refresh")}</Button>
      <Button type="button" size="sm" variant="outline" disabled={state.busy || catalogBusy} onClick={() => void refreshCatalog()}>{t("pluginRuntime.refreshCatalog")}</Button>
      {state.busy && <span role="status">{t("ui.loading")}</span>}
    </div>
    {(state.error || formError) && <p role="alert" className="whitespace-pre-wrap break-all text-destructive">{state.error || formError}</p>}
    {!state.fresh && state.observation && <p role="status">{t("pluginRuntime.staleRead")}</p>}
    {state.fresh && !runtime && <p role="status">{t("pluginRuntime.noRuntime")}</p>}
    {runtime && <div className="space-y-1 break-all">
      <p>{t("pluginRuntime.parent")}: <code>{runtime.runtimeId}</code></p>
      <p>{formatMessage(t("pluginRuntime.usage"), { ...runtime.resources })}</p>
    </div>}
    {state.pending && <div className="space-y-2 rounded-md border p-3">
      <p role="alert">{t("pluginRuntime.unknown")}</p>
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(state.pending, null, 2)}</pre>
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" disabled={state.busy} onClick={() => { if (state.pending) void state.execute(state.pending); }}>{t("pluginRuntime.retry")}</Button>
        <Button type="button" size="sm" variant="outline" disabled={state.busy || !state.fresh}
          onClick={() => setConfirmation({ command: null, warning: "pluginRuntime.discardWarning" })}>{t("pluginRuntime.discardTracking")}</Button>
      </div>
    </div>}
    {receipt && <div className="space-y-2 rounded-md border p-3">
      <p className="font-medium">{t("pluginRuntime.receipt")}: <code>{receipt.phase}</code></p>
      <p className="break-all">{receipt.request.pluginId} / {receipt.request.purpose} / {receipt.request.updateId}</p>
      <p>{t("pluginRuntime.receiptHint")}</p>
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(receipt.request, null, 2)}</pre>
      {(receipt.error || receipt.persistenceError) && <p role="alert" className="whitespace-pre-wrap break-all text-destructive">{[receipt.error, receipt.persistenceError].filter(Boolean).join("\n")}</p>}
      <div className="flex flex-wrap gap-2">
        {(receipt.phase === "prepared" || receipt.phase === "applied") && <Button type="button" size="sm" disabled={!canWrite}
          onClick={() => void state.execute({ kind: "resume", updateId: receipt.request.updateId })}>{t("pluginRuntime.resume")}</Button>}
        {receipt.phase === "prepared" && <Button type="button" size="sm" variant="outline" disabled={!canWrite}
          onClick={() => void state.execute({ kind: "cancel", updateId: receipt.request.updateId })}>{t("pluginRuntime.cancelUpdate")}</Button>}
        {receipt.phase !== "prepared" && receipt.phase !== "applied" && <Button type="button" size="sm" variant="outline" disabled={!canWrite}
          onClick={() => void state.execute({ kind: "forget", updateId: receipt.request.updateId })}>{t("pluginRuntime.ack")}</Button>}
      </div>
    </div>}
    {catalog && <>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <label htmlFor={fieldPrefix + "-plugin"}>{t("pluginRuntime.plugin")}</label>
          <Select value={pluginId} onValueChange={setPluginId}>
            <SelectTrigger id={fieldPrefix + "-plugin"}><SelectValue placeholder={t("pluginRuntime.plugin")} /></SelectTrigger>
            <SelectContent>{pluginIds.map((id) => <SelectItem key={id} value={id}>{id}</SelectItem>)}</SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <label htmlFor={fieldPrefix + "-purpose"}>{t("pluginRuntime.purpose")}</label>
          <Select value={purpose} onValueChange={(value) => setPurpose(value as PluginCapacityPurpose)}>
            <SelectTrigger id={fieldPrefix + "-purpose"}><SelectValue /></SelectTrigger>
            <SelectContent>{PLUGIN_CAPACITY_PURPOSES.map((value) => <SelectItem key={value} value={value}>{value}</SelectItem>)}</SelectContent>
          </Select>
        </div>
      </div>
      {pluginId && !selected && <div className="flex flex-wrap items-center gap-2">
        <span>{t("pluginRuntime.absent")}</span>
        <Button type="button" size="sm" disabled={!canRegister} onClick={() => void state.execute({ kind: "register", request: {
          registrationId: crypto.randomUUID(), runtimeId: catalog.runtimeId, expectedRegistryRevision: catalog.registryRevision, pluginId, purpose,
        } })}>{t("pluginRuntime.register")}</Button>
      </div>}
      {selected && <div className="space-y-2 rounded-md border p-3">
        <p className="break-all"><code>{selected.snapshot.capacity_id}</code> / <code>{selected.policyRevision}</code></p>
        {selected.managementRequest && <p className="break-all">registrationId: <code>{selected.managementRequest.registrationId}</code></p>}
        <p>{t(selected.budgetSource === "host_override" ? "pluginRuntime.override" : "pluginRuntime.inherited")} · {t(selected.snapshot.config.resources.kind === "shared" ? "pluginRuntime.shared" : "pluginRuntime.dedicated")}</p>
        <p>{formatMessage(t("pluginRuntime.usage"), { ...selected.snapshot.resources })}</p>
        <dl className="grid gap-2 sm:grid-cols-2">
          {Object.entries({
            min_resident_vms: selected.snapshot.config.resources.min_resident_vms,
            max_resident_vms: selected.snapshot.config.resources.max_resident_vms,
            max_running_calls: selected.snapshot.config.resources.max_running_calls,
            max_queued_calls: selected.snapshot.config.max_queued_calls,
            max_queued_bytes: selected.snapshot.config.max_queued_bytes,
          }).map(([key, value]) => <div key={key}>
            <dt className="text-muted-foreground">{t(numericLabels[key as keyof PluginCapacityDraft["numbers"]])}</dt>
            <dd>{value}</dd>
          </div>)}
        </dl>
        <p>{formatMessage(t("pluginRuntime.work"), {
          reserved: selected.snapshot.committed_resident_vms, active: selected.snapshot.active_operations,
          calls: selected.snapshot.queued_calls, bytes: selected.snapshot.queued_bytes,
          pools: selected.snapshot.retained_pools, preparing: selected.preparingMembers,
        })}</p>
        {selected.pendingConvergence && <p role="status">{t("pluginRuntime.convergence")}</p>}
        {selected.snapshot.closing && <p role="status">{t("pluginRuntime.closing")}</p>}
        {selected.retirementRequested && <p role="status">{t("pluginRuntime.retiring")}</p>}
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" disabled={!canEdit || draft !== null}
            onClick={() => { setDraft(createCapacityDraft(selected)); setFormError(""); }}>{t("pluginRuntime.edit")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canEdit}
            onClick={() => setConfirmation({ warning: "pluginRuntime.resetWarning", command: { kind: "update", request: {
              ...capacityPredecessor(selected), updateId: crypto.randomUUID(), config: null,
            } } })}>{t("pluginRuntime.reset")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canWrite}
            onClick={() => setConfirmation({ warning: "pluginRuntime.closeWarning", command: { kind: "control", request: {
              ...capacityPredecessor(selected), action: "close",
            } } })}>{t("pluginRuntime.close")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canWrite || !selected.snapshot.closing || selected.preparingMembers > 0 || receipt?.phase === "prepared" || receipt?.phase === "applied"}
            onClick={() => setConfirmation({ warning: "pluginRuntime.forgetWarning", command: { kind: "control", request: {
              ...capacityPredecessor(selected), action: "forget",
            } } })}>{t("pluginRuntime.forget")}</Button>
        </div>
      </div>}
    </>}
    {draft && <div className="space-y-3 rounded-md border p-3">
      <p className="font-medium">{t("pluginRuntime.draft")}: {draft.predecessor.pluginId} / {draft.predecessor.purpose}</p>
      <p className="break-all"><code>{draft.predecessor.capacityId}</code> / <code>{draft.predecessor.expectedRevision}</code></p>
      {!draftCurrent && <p role="alert">{t("pluginRuntime.staleDraft")}</p>}
      <div className="space-y-1">
        <label htmlFor={fieldPrefix + "-kind"}>{t("pluginRuntime.kind")}</label>
        <Select value={draft.kind} onValueChange={(kind) => setDraft({ ...draft, kind: kind as PluginCapacityDraft["kind"] })}>
          <SelectTrigger id={fieldPrefix + "-kind"}><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="shared">{t("pluginRuntime.shared")}</SelectItem><SelectItem value="dedicated">{t("pluginRuntime.dedicated")}</SelectItem></SelectContent>
        </Select>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        {(Object.keys(numericLabels) as Array<keyof PluginCapacityDraft["numbers"]>).map((key) => <div className="space-y-1" key={key}>
          <label htmlFor={fieldPrefix + key}>{t(numericLabels[key])}</label>
          <Input id={fieldPrefix + key} inputMode="numeric" value={draft.numbers[key]}
            onChange={(event) => setDraft({ ...draft, numbers: { ...draft.numbers, [key]: event.target.value } })} />
        </div>)}
      </div>
      <p className="text-muted-foreground">{t("pluginRuntime.boundsHint")}</p>
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" disabled={!canWrite || !draftCurrent || !!receipt} onClick={submit}>{t("pluginRuntime.save")}</Button>
        <Button type="button" size="sm" variant="outline" disabled={state.busy} onClick={() => { setDraft(null); setFormError(""); }}>{t("pluginRuntime.discardDraft")}</Button>
      </div>
    </div>}
    <AlertDialog open={confirmation !== null} onOpenChange={(open) => { if (!open) setConfirmation(null); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("pluginRuntime.confirm")}</AlertDialogTitle>
          <AlertDialogDescription>{confirmation && t(confirmation.warning)}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation?.command && <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">{JSON.stringify(confirmation.command, null, 2)}</pre>}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("skills.plugins.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={state.busy || !state.fresh || (!!confirmation?.command && !!state.pending)}
            onClick={() => {
              if (!confirmation) return;
              if (confirmation.command) void state.execute(confirmation.command);
              else state.discardTracking();
              setConfirmation(null);
            }}>{t("skills.plugins.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>;
}
