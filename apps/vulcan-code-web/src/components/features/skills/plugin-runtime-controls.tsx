import * as React from "react";
import { MoreHorizontal, RefreshCw, SlidersHorizontal } from "lucide-react";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button, IconButton } from "@/ui-kit/primitives/button";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";
import { Input } from "@/ui-kit/primitives/input";
import { Progress } from "@/ui-kit/primitives/progress";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/ui-kit/primitives/dropdown-menu";
import { Field, FieldDescription, FieldLabel } from "@/ui-kit/primitives/field";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary } from "@/types";
import { PLUGIN_CAPACITY_PURPOSES, type PluginCapacityPurpose, type PluginCapacityStatus } from "@/types/plugin-runtime";
import type { PluginRuntimeCommand } from "@/service/plugin-runtime-service";
import { capacityPredecessor, createCapacityDraft, draftStillCurrent, readCapacityDraft, type PluginCapacityDraft } from "./plugin-capacity-draft";
import { PageEmptyState, PageFacts, PageHint, PageItem, PageItemList, PageNotice, PageToolbar } from "@/components/page";
import { TechnicalDetails } from "./plugin-maintenance-section";
import { capacityActionLabels, capacityPurposeLabels, capacityReceiptLabels } from "./plugin-policy-labels";
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
  // pluginId limits the list to one plugin when opened from that plugin's card.
  // pluginId 从插件卡片打开时将列表限定为单个插件。
  pluginId?: string;
}

// numericFields lists every editable native number with its label and plain explanation, in form order.
// numericFields 按表单顺序列出每个可编辑原生数字及其标签与平实说明。
const numericFields: ReadonlyArray<readonly [keyof PluginCapacityDraft["numbers"], TranslationKey, TranslationKey]> = [
  ["min_resident_vms", "pluginRuntime.min", "pluginRuntime.minHint"],
  ["max_resident_vms", "pluginRuntime.max", "pluginRuntime.maxHint"],
  ["max_running_calls", "pluginRuntime.running", "pluginRuntime.runningHint"],
  ["max_queued_calls", "pluginRuntime.queueCalls", "pluginRuntime.queueCallsHint"],
  ["max_queued_bytes", "pluginRuntime.queueBytes", "pluginRuntime.queueBytesHint"],
];

// Confirmation holds one reviewed command, or null for discarding this view's uncertain tracking.
// Confirmation 持有单个已审阅命令；null 表示放弃本视图的不确定追踪。
interface Confirmation {
  title: TranslationKey;
  warning: TranslationKey;
  command: PluginRuntimeCommand | null;
}

// PluginRuntimeControls shows how many isolated runtimes each plugin may use and lets the user adjust the limits.
// PluginRuntimeControls 显示每个插件可使用的隔离运行环境数量，并允许用户调整上限。
// Every write is fenced by the observed predecessor; an uncertain outcome is retained for explicit retry.
// 每次写入都受观测前驱约束；不确定结果会保留以便明确重试。
export function PluginRuntimeControls(props: PluginRuntimeControlProps) {
  const state = usePluginRuntimeManagement();
  const { t } = useI18n();
  const [draft, setDraft] = React.useState<PluginCapacityDraft | null>(null);
  const [formError, setFormError] = React.useState("");
  const [confirmation, setConfirmation] = React.useState<Confirmation | null>(null);
  const [catalogBusy, setCatalogBusy] = React.useState(false);
  const catalogBusyRef = React.useRef(false);
  const [registerPlugin, setRegisterPlugin] = React.useState(props.pluginId ?? "");
  const [registerPurpose, setRegisterPurpose] = React.useState<PluginCapacityPurpose | "">("");
  const fieldPrefix = React.useId();
  const catalog = state.observation?.catalog;
  const runtime = state.observation?.runtime;
  const receipt = runtime?.policyUpdate;
  const nameOf = (pluginId: string) => props.plugins.find((plugin) => plugin.id === pluginId)?.name ?? pluginId;
  // groups lists capacities per plugin; retained owners remain visible after uninstall.
  // groups 按插件列出容量；卸载后保留的所有者仍可见。
  const capacities = (catalog?.capacities ?? []).filter((capacity) => props.pluginId === undefined || capacity.snapshot.plugin_id === props.pluginId);
  const groups = [...new Set(capacities.map((capacity) => capacity.snapshot.plugin_id))].sort()
    .map((pluginId) => [pluginId, capacities.filter((capacity) => capacity.snapshot.plugin_id === pluginId)] as const);
  const luaPlugins = props.plugins.filter((plugin) => plugin.extension?.hasLuaRuntime === true && (props.pluginId === undefined || plugin.id === props.pluginId));
  const draftOwner = draft ? catalog?.capacities.find((capacity) => capacity.snapshot.plugin_id === draft.predecessor.pluginId && capacity.purpose === draft.predecessor.purpose) : undefined;
  const draftCurrent = draft !== null && draftStillCurrent(draft, draftOwner);
  const canWrite = state.fresh && !state.busy && !state.pending && !catalogBusy;
  const canEdit = (capacity: PluginCapacityStatus) => canWrite && !receipt && !capacity.snapshot.closing && !capacity.retirementRequested;
  // unregistered lists purposes the selected plugin has no capacity entry for yet.
  // unregistered 列出所选插件尚无容量条目的用途。
  const unregistered = PLUGIN_CAPACITY_PURPOSES.filter((purpose) => !(catalog?.capacities ?? [])
    .some((capacity) => capacity.snapshot.plugin_id === registerPlugin && capacity.purpose === purpose));
  const canRegister = canWrite && catalog !== undefined && registerPurpose !== "" && unregistered.includes(registerPurpose)
    && luaPlugins.some((plugin) => plugin.id === registerPlugin);

  // submit validates a full draft before allocating a transaction UUID; the dialog closes once the command is sent.
  // submit 在分配事务 UUID 前校验完整草稿；命令发出后关闭对话框。
  const submit = (): void => {
    if (!draft || !draftCurrent || !canWrite || receipt) return;
    try {
      // config contains every current native field, with no inherited values guessed by the browser.
      // config 包含全部当前原生字段，浏览器不猜测继承值。
      const config = readCapacityDraft(draft);
      setFormError("");
      setDraft(null);
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

  const usage = runtime?.resources;
  const receiptActive = receipt !== undefined && receipt !== null && (receipt.phase === "prepared" || receipt.phase === "applied");
  return <div className="plugin-capacity">
    <PageToolbar actions={<Button disabled={state.busy || catalogBusy} onClick={() => void state.refresh()} size="sm" type="button" variant="outline">
      <RefreshCw data-icon="inline-start" />{t("pluginRuntime.refresh")}</Button>}>
      {state.busy && <PageHint role="status">{t("ui.loading")}</PageHint>}
    </PageToolbar>
    {(state.error || formError) && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{state.error || formError}</span></PageNotice>}
    {!state.fresh && state.observation && !state.busy && <PageNotice tone="info">{t("pluginRuntime.staleRead")}</PageNotice>}
    {state.fresh && !runtime && <PageNotice tone="info">{t("pluginRuntime.noRuntime")}</PageNotice>}
    {state.pending && <PageNotice actions={<>
      <Button disabled={state.busy || !state.fresh} onClick={() => setConfirmation({ title: "pluginRuntime.discardTracking", warning: "pluginRuntime.discardWarning", command: null })}
        size="sm" type="button" variant="ghost">{t("pluginRuntime.discardTracking")}</Button>
      <Button disabled={state.busy} onClick={() => { if (state.pending) void state.execute(state.pending); }} size="sm" type="button">{t("pluginRuntime.retry")}</Button>
    </>} tone="warning">
      <p>{t("pluginRuntime.unknown")}</p>
      <TechnicalDetails entries={[]} extra={<pre>{JSON.stringify(state.pending, null, 2)}</pre>} />
    </PageNotice>}
    {receipt && <PageNotice actions={(state.observation?.receiptActions ?? []).map(({ action }) => {
      if (action === "abort") return null;
      const [label, variant] = capacityActionLabels[action];
      return <Button disabled={!canWrite} key={action} onClick={() => void state.execute({ kind: action, updateId: receipt.request.updateId })}
        size="sm" type="button" variant={variant}>{t(label)}</Button>;
    })} tone={receipt.phase === "committed" ? "success" : receiptActive ? "warning" : "error"}>
      <p>{formatMessage(t(capacityReceiptLabels[receipt.phase]), { name: nameOf(receipt.request.pluginId), purpose: t(capacityPurposeLabels[receipt.request.purpose]) })}</p>
      {(receipt.error || receipt.persistenceError) && <p className="whitespace-pre-wrap break-all">{[receipt.error, receipt.persistenceError].filter(Boolean).join("\n")}</p>}
      <TechnicalDetails entries={[["updateId", <code key="id">{receipt.request.updateId}</code>], ["phase", <code key="phase">{receipt.phase}</code>]]}
        extra={<pre>{JSON.stringify(receipt.request, null, 2)}</pre>} />
    </PageNotice>}
    {usage && props.pluginId === undefined && <PageFacts items={[
      { key: "resident", label: t("pluginRuntime.stat.resident"), value: usage.resident },
      { key: "running", label: t("pluginRuntime.stat.running"), value: usage.running },
      { key: "idle", label: t("pluginRuntime.stat.idle"), value: usage.idle },
      { key: "creating", label: t("pluginRuntime.stat.creating"), value: usage.creating },
      { key: "retiring", label: t("pluginRuntime.stat.retiring"), value: usage.retiring, tone: usage.retiring > 0 ? "attention" : "neutral" },
    ]} layout="tiles" />}
    {catalog && groups.length === 0 && <PageEmptyState title={t("pluginRuntime.emptyList")} />}
    {groups.map(([pluginId, rows]) => <PageItemList aria-label={nameOf(pluginId)} heading={props.pluginId === undefined ? nameOf(pluginId) : undefined}
      key={pluginId} role="group">
      {rows.map((capacity) => <CapacityRow canEdit={canEdit(capacity)} canWrite={canWrite} capacity={capacity} key={capacity.snapshot.capacity_id}
        onConfirm={setConfirmation} onEdit={() => { setDraft(createCapacityDraft(capacity)); setFormError(""); }} receiptActive={receiptActive} />)}
    </PageItemList>)}
    {catalog && luaPlugins.length > 0 && <InlineDisclosure label={t("pluginRuntime.registerToggle")}>
      <div className="plugin-capacity-register">
        <PageHint>{t("pluginRuntime.registerHint")}</PageHint>
        <div className="plugin-capacity-register-fields">
          {props.pluginId === undefined && <Field>
            <FieldLabel htmlFor={`${fieldPrefix}-register-plugin`}>{t("pluginRuntime.plugin")}</FieldLabel>
            <Select onValueChange={(value) => { setRegisterPlugin(value); setRegisterPurpose(""); }} value={registerPlugin}>
              <SelectTrigger id={`${fieldPrefix}-register-plugin`}><SelectValue placeholder={t("pluginRuntime.plugin")} /></SelectTrigger>
              <SelectContent>{luaPlugins.map((plugin) => <SelectItem key={plugin.id} value={plugin.id}>{plugin.name}</SelectItem>)}</SelectContent>
            </Select>
          </Field>}
          <Field>
            <FieldLabel htmlFor={`${fieldPrefix}-register-purpose`}>{t("pluginRuntime.purposeField")}</FieldLabel>
            <Select disabled={!registerPlugin} onValueChange={(value) => setRegisterPurpose(value as PluginCapacityPurpose)} value={registerPurpose}>
              <SelectTrigger id={`${fieldPrefix}-register-purpose`}><SelectValue placeholder={t("pluginRuntime.purposeField")} /></SelectTrigger>
              <SelectContent>{unregistered.map((purpose) => <SelectItem key={purpose} value={purpose}>{t(capacityPurposeLabels[purpose])}</SelectItem>)}</SelectContent>
            </Select>
          </Field>
          <Button disabled={!canRegister} onClick={() => {
            if (!catalog || registerPurpose === "") return;
            void state.execute({ kind: "register", request: { registrationId: crypto.randomUUID(), runtimeId: catalog.runtimeId,
              expectedRegistryRevision: catalog.registryRevision, pluginId: registerPlugin, purpose: registerPurpose } });
            setRegisterPurpose("");
          }} size="sm" type="button">{t("pluginRuntime.register")}</Button>
        </div>
      </div>
    </InlineDisclosure>}
    <PageToolbar actions={<Button disabled={state.busy || catalogBusy} onClick={() => void refreshCatalog()} size="sm" type="button" variant="ghost">
      {t("pluginRuntime.refreshCatalog")}</Button>}>
      <PageHint>{t("pluginRuntime.refreshCatalogHint")}</PageHint>
    </PageToolbar>
    <Dialog onOpenChange={(open) => { if (!open) { setDraft(null); setFormError(""); } }} open={draft !== null}>
      <DialogContent size="md" scrollMode="fixed-footer">
        <DialogHeader>
          <DialogTitle>{draft ? formatMessage(t("pluginRuntime.editTitle"), { name: nameOf(draft.predecessor.pluginId), purpose: t(capacityPurposeLabels[draft.predecessor.purpose]) }) : ""}</DialogTitle>
          <DialogDescription>{t("pluginRuntime.editDescription")}</DialogDescription>
        </DialogHeader>
        {draft && <div className="plugin-capacity-form">
          {!draftCurrent && <PageNotice tone="warning">{t("pluginRuntime.staleDraft")}</PageNotice>}
          {formError && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{formError}</span></PageNotice>}
          <Field>
            <FieldLabel htmlFor={`${fieldPrefix}-kind`}>{t("pluginRuntime.kind")}</FieldLabel>
            <Select onValueChange={(kind) => setDraft({ ...draft, kind: kind as PluginCapacityDraft["kind"],
              numbers: kind === "shared" ? { ...draft.numbers, min_resident_vms: "0" } : draft.numbers })} value={draft.kind}>
              <SelectTrigger id={`${fieldPrefix}-kind`}><SelectValue /></SelectTrigger>
              <SelectContent><SelectItem value="shared">{t("pluginRuntime.shared")}</SelectItem><SelectItem value="dedicated">{t("pluginRuntime.dedicated")}</SelectItem></SelectContent>
            </Select>
            <FieldDescription>{t(draft.kind === "shared" ? "pluginRuntime.sharedHint" : "pluginRuntime.dedicatedHint")}</FieldDescription>
          </Field>
          <div className="plugin-capacity-form-grid">
            {numericFields.map(([key, label, hint]) => <Field key={key}>
              <FieldLabel htmlFor={fieldPrefix + key}>{t(label)}</FieldLabel>
              <Input disabled={key === "min_resident_vms" && draft.kind === "shared"} id={fieldPrefix + key} inputMode="numeric"
                onChange={(event) => setDraft({ ...draft, numbers: { ...draft.numbers, [key]: event.target.value } })} value={draft.numbers[key]} />
              <FieldDescription>{t(key === "min_resident_vms" && draft.kind === "shared" ? "pluginRuntime.minSharedHint" : hint)}</FieldDescription>
            </Field>)}
          </div>
          <PageHint>{t("pluginRuntime.boundsHint")}</PageHint>
        </div>}
        <DialogFooter>
          <Button disabled={state.busy} onClick={() => { setDraft(null); setFormError(""); }} type="button" variant="outline">{t("pluginRuntime.discardDraft")}</Button>
          <Button disabled={!canWrite || !draftCurrent || !!receipt} onClick={submit} type="button">{t("pluginRuntime.save")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    <AlertDialog onOpenChange={(open) => { if (!open) setConfirmation(null); }} open={confirmation !== null}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{confirmation && t(confirmation.title)}</AlertDialogTitle>
          <AlertDialogDescription>{confirmation && t(confirmation.warning)}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmation?.command && <TechnicalDetails entries={[]} extra={<pre>{JSON.stringify(confirmation.command, null, 2)}</pre>} />}
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

// CapacityRow renders one plugin purpose as badges plus usage meters, with adjust and overflow actions.
// CapacityRow 以徽章及用量条渲染单个插件用途，并提供调整与更多操作。
function CapacityRow(props: {
  capacity: PluginCapacityStatus; canEdit: boolean; canWrite: boolean; receiptActive: boolean;
  onEdit: () => void; onConfirm: (confirmation: Confirmation) => void;
}) {
  const { t } = useI18n();
  const capacity = props.capacity;
  const snapshot = capacity.snapshot;
  const config = snapshot.config;
  const label = t(capacityPurposeLabels[capacity.purpose]);
  // meters compare live usage with the configured limit; a zero limit renders an empty bar.
  // meters 将实时用量与配置上限比较；上限为零时显示空条。
  const meters: ReadonlyArray<readonly [TranslationKey, number, number]> = [
    ["pluginRuntime.meter.vms", snapshot.resources.resident, config.resources.max_resident_vms],
    ["pluginRuntime.meter.running", snapshot.active_operations, config.resources.max_running_calls],
    ["pluginRuntime.meter.queue", snapshot.queued_calls, config.max_queued_calls],
  ];
  const canForget = props.canWrite && snapshot.closing && capacity.preparingMembers === 0 && !props.receiptActive;
  const attention = snapshot.closing || capacity.pendingConvergence || capacity.retirementRequested;
  return <PageItem actions={<>
    <Button disabled={!props.canEdit} onClick={props.onEdit} size="sm" type="button" variant="outline">
      <SlidersHorizontal data-icon="inline-start" />{t("pluginRuntime.edit")}</Button>
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton aria-label={formatMessage(t("pluginRuntime.moreActions"), { purpose: label })} type="button"><MoreHorizontal size={16} /></IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem disabled={!props.canEdit || capacity.budgetSource !== "host_override"} onSelect={() => props.onConfirm({
          title: "pluginRuntime.reset", warning: "pluginRuntime.resetWarning",
          command: { kind: "update", request: { ...capacityPredecessor(capacity), updateId: crypto.randomUUID(), config: null } },
        })}>{t("pluginRuntime.reset")}</DropdownMenuItem>
        <DropdownMenuItem disabled={!props.canWrite || snapshot.closing} onSelect={() => props.onConfirm({
          title: "pluginRuntime.close", warning: "pluginRuntime.closeWarning",
          command: { kind: "control", request: { ...capacityPredecessor(capacity), action: "close" } },
        })}>{t("pluginRuntime.close")}</DropdownMenuItem>
        <DropdownMenuItem disabled={!canForget} onSelect={() => props.onConfirm({
          title: "pluginRuntime.forget", warning: "pluginRuntime.forgetWarning",
          command: { kind: "control", request: { ...capacityPredecessor(capacity), action: "forget" } },
        })}>{t("pluginRuntime.forget")}</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  </>} aria-label={label} meta={<>
    <Badge variant="outline">{t(config.resources.kind === "shared" ? "pluginRuntime.shared" : "pluginRuntime.dedicated")}</Badge>
    <Badge variant={capacity.budgetSource === "host_override" ? "secondary" : "ghost"}>
      {t(capacity.budgetSource === "host_override" ? "pluginRuntime.override" : "pluginRuntime.inherited")}</Badge>
    {capacity.pendingConvergence && <Badge variant="warning">{t("pluginRuntime.convergence")}</Badge>}
    {snapshot.closing && <Badge variant="warning">{t("pluginRuntime.closing")}</Badge>}
    {capacity.retirementRequested && <Badge variant="warning">{t("pluginRuntime.retiring")}</Badge>}
  </>} title={label} tone={attention ? "attention" : "neutral"}>
    <div className="plugin-capacity-meters">
      {meters.map(([key, value, max]) => <div className="plugin-capacity-meter" key={key}>
        <div className="plugin-capacity-meter-label"><span>{t(key)}</span><span>{value} / {max}</span></div>
        <Progress aria-label={t(key)} max={max > 0 ? max : 1} value={max > 0 ? value : 0} />
      </div>)}
    </div>
    <PageHint>{formatMessage(t("pluginRuntime.rowSummary"), {
      min: config.resources.min_resident_vms, idle: snapshot.resources.idle, creating: snapshot.resources.creating,
    })}</PageHint>
    <TechnicalDetails entries={[
      ["capacityId", <code key="capacity">{snapshot.capacity_id}</code>],
      ["policyRevision", <code key="revision">{capacity.policyRevision}</code>],
      ["committedResidentVms", <code key="committed">{snapshot.committed_resident_vms}</code>],
      ["queuedBytes", <code key="bytes">{snapshot.queued_bytes} / {config.max_queued_bytes}</code>],
      ["retainedPools", <code key="pools">{snapshot.retained_pools}</code>],
      ["preparingMembers", <code key="preparing">{capacity.preparingMembers}</code>],
      ...(capacity.managementRequest ? [["registrationId", <code key="registration">{capacity.managementRequest.registrationId}</code>] as const] : []),
    ]} />
  </PageItem>;
}
