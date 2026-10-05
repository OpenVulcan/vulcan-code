import * as React from "react";
import { Flame, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Button, IconButton } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { PluginSummary } from "@/types";
import type { PluginAutomaticPrewarmRule, PluginPrewarmPolicyPhase } from "@/types/plugin-prewarm-policy";
import { PageEmptyState, PageHint, PageItem, PageItemList, PageNotice, PageToolbar } from "@/components/page";
import { TechnicalDetails } from "./plugin-maintenance-section";
import { policyReceiptAllows, usePluginPrewarmPolicy } from "./use-plugin-prewarm-policy";

// RuleDraft keeps editable text separate from the validated wire count and stable row identity.
// RuleDraft 将可编辑文本与已校验线协议数量及稳定行身份分离。
interface RuleDraft {
  // id is local editor identity only and never crosses the wire.
  // id 仅为本地编辑身份，绝不跨越线协议。
  id: string;
  // pluginId names the exact package, including intentionally unavailable configured packages.
  // pluginId 指定精确包，包括有意配置的暂不可用包。
  pluginId: string;
  // toolId names the globally unique tool without any inferred alias.
  // toolId 指定全局唯一工具，不推断别名。
  toolId: string;
  // target stores explicit user input; a blank row grants no default capacity.
  // target 保存明确用户输入；空行不授予默认容量。
  target: string;
}

// PolicyDraft freezes the observed predecessor so refresh cannot silently rebase unsaved edits.
// PolicyDraft 冻结已观测前驱，防止刷新静默重定位未保存编辑。
interface PolicyDraft {
  runtimeId: string;
  revision: string;
  rows: RuleDraft[];
}

// draftRules validates explicit positive safe counts and unique tool ownership; null means invalid input.
// draftRules 校验明确正安全整数及唯一工具归属；空值表示输入无效。
function draftRules(rows: RuleDraft[]): PluginAutomaticPrewarmRule[] | null {
  const seen = new Set<string>();
  const rules: PluginAutomaticPrewarmRule[] = [];
  for (const row of rows) {
    if (!row.pluginId.trim() || !row.toolId.trim() || seen.has(row.toolId) || !/^[1-9][0-9]*$/.test(row.target)) return null;
    const targetIdle = Number(row.target);
    if (!Number.isSafeInteger(targetIdle)) return null;
    seen.add(row.toolId); rules.push({ pluginId: row.pluginId, toolId: row.toolId, targetIdle });
  }
  return rules;
}

// receiptLabels explains the last rule publication in plain words.
// receiptLabels 以平实语言说明上一次规则发布。
const receiptLabels: Record<PluginPrewarmPolicyPhase, TranslationKey> = {
  prepared: "pluginPrewarmPolicy.receipt.prepared", publishing: "pluginPrewarmPolicy.receipt.publishing",
  committed: "pluginPrewarmPolicy.receipt.committed", cancelled: "pluginPrewarmPolicy.receipt.cancelled",
  interrupted: "pluginPrewarmPolicy.receipt.interrupted",
};

// PluginPrewarmPolicyControls lists the automatic prewarm rules and edits them as one full replacement.
// PluginPrewarmPolicyControls 列出自动预热规则，并以整体替换方式编辑。
// Reading happens on mount; no write occurs until the user saves an explicit draft.
// 挂载时读取；在用户保存明确草稿前不发生写入。
export function PluginPrewarmPolicyControls(props: { plugins: PluginSummary[] }) {
  const management = usePluginPrewarmPolicy();
  const { observation, busy } = management;
  const { status, fresh, pending, acknowledged, error } = observation;
  const { t } = useI18n();
  const prefix = React.useId();
  const [draft, setDraft] = React.useState<PolicyDraft | null>(null);
  // refreshRef lets the mount effect call the latest reader exactly once per mount.
  // refreshRef 使挂载效果每次挂载仅调用一次最新读取器。
  const refreshRef = React.useRef(management.refresh);
  React.useEffect(() => { refreshRef.current = management.refresh; });
  React.useEffect(() => { void refreshRef.current(); }, []);
  // writable requires an atomic current read with no competing or uncertain transaction.
  // writable 要求原子当前读取，且没有竞争或未知事务。
  const writable = fresh && !busy && pending === null && status !== null && status.update === null && !status.capacityUpdateRetained;
  const currentDraft = draft !== null && status !== null && draft.runtimeId === status.runtimeId && draft.revision === status.revision;
  const rules = draft === null ? null : draftRules(draft.rows);
  // receipt prefers a write acknowledged after the last complete read, so a failed follow-up read never hides the outcome.
  // receipt 优先采用最后完整读取之后确认的写入，使后续读取失败时也不会隐藏结果。
  const receipt = !fresh && acknowledged ? acknowledged : status?.update;
  const luaPlugins = props.plugins.filter((plugin) => plugin.extension?.hasLuaRuntime === true);
  const nameOf = (pluginId: string) => props.plugins.find((plugin) => plugin.id === pluginId)?.name ?? pluginId;

  // edit replaces the local draft from the last observed effective rules; addFirst starts with one blank row.
  // edit 以最后观测的生效规则替换本地草稿；addFirst 以一个空行开始。
  // blankRow starts a new rule; the plugin is preselected only when exactly one Lua plugin exists, and stays visible and editable.
  // blankRow 开始新规则；仅当恰有一个 Lua 插件时预选插件，且保持可见、可修改。
  const blankRow = (): RuleDraft => ({ id: crypto.randomUUID(), pluginId: luaPlugins.length === 1 ? luaPlugins[0].id : "", toolId: "", target: "1" });
  const edit = (addFirst: boolean): void => {
    if (!writable || !status) return;
    const rows: RuleDraft[] = status.rules.map((rule) => ({ id: crypto.randomUUID(), pluginId: rule.pluginId, toolId: rule.toolId, target: String(rule.targetIdle) }));
    if (addFirst) rows.push(blankRow());
    setDraft({ runtimeId: status.runtimeId, revision: status.revision, rows });
  };
  // update edits one stable local row without changing the original predecessor.
  // update 编辑单个稳定本地行，不改变原前驱。
  const update = (id: string, changes: Partial<Pick<RuleDraft, "pluginId" | "toolId" | "target">>): void => {
    setDraft((original) => original === null ? null : { ...original, rows: original.rows.map((row) => row.id === id ? { ...row, ...changes } : row) });
  };
  // save signs one caller UUID for the frozen draft and closes the editor; the receipt reports the outcome.
  // save 为冻结草稿签发单个调用方 UUID 并关闭编辑器；回执报告结果。
  const save = (): void => {
    if (!writable || !currentDraft || !draft || rules === null) return;
    void management.execute({ kind: "begin", request: { updateId: crypto.randomUUID(), runtimeId: draft.runtimeId,
      expectedRevision: draft.revision, rules } });
    setDraft(null);
  };

  return <div className="plugin-prewarm">
    {error && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{error}</span></PageNotice>}
    {status === null && busy && <PageHint role="status">{t("ui.loading")}</PageHint>}
    {status !== null && !fresh && !busy && <PageNotice actions={<Button onClick={() => void management.refresh()} size="sm" type="button" variant="outline">
      {t("pluginRuntime.refresh")}</Button>} tone="info">{t("pluginPrewarmPolicy.stale")}</PageNotice>}
    {status?.capacityUpdateRetained === true && <PageNotice tone="warning">{t("pluginPrewarmPolicy.capacityBlocked")}</PageNotice>}
    {pending && <PageNotice actions={<Button disabled={busy || !fresh} onClick={management.acknowledgeUncertainty} size="sm" type="button" variant="outline">
      {t("pluginPrewarmPolicy.acknowledge")}</Button>} tone="warning">
      <p>{t("pluginPrewarmPolicy.unknown")}</p>
      <TechnicalDetails entries={[]} extra={<pre>{JSON.stringify(pending, null, 2)}</pre>} />
    </PageNotice>}
    {receipt && <PageNotice actions={<>
      {policyReceiptAllows(receipt, "cancel") && <Button disabled={busy || !fresh || pending !== null} onClick={() => void management.execute({ kind: "cancel", request: receipt.request })}
        size="sm" type="button" variant="ghost">{t("pluginPrewarmPolicy.cancel")}</Button>}
      {((receipt.phase !== "committed" && receipt.phase !== "cancelled") || receipt.persistenceError !== undefined) && <Button disabled={busy || !fresh || pending !== null}
        onClick={() => void management.execute({ kind: "resume", request: receipt.request })} size="sm" type="button">{t("pluginPrewarmPolicy.resume")}</Button>}
      {policyReceiptAllows(receipt, "forget") && <Button disabled={busy || !fresh || pending !== null} onClick={() => void management.execute({ kind: "forget", request: receipt.request })}
        size="sm" type="button" variant="outline">{t("pluginPrewarmPolicy.forget")}</Button>}
    </>} tone={receipt.phase === "committed" ? "success" : receipt.phase === "interrupted" ? "error" : receipt.phase === "cancelled" ? "info" : "warning"}>
      <p>{t(receiptLabels[receipt.phase])}</p>
      {(receipt.error || receipt.persistenceError) && <p className="whitespace-pre-wrap break-all">{[receipt.error, receipt.persistenceError].filter(Boolean).join("\n")}</p>}
      <TechnicalDetails entries={[["updateId", <code key="id">{receipt.request.updateId}</code>], ["phase", <code key="phase">{receipt.phase}</code>]]}
        extra={<pre>{JSON.stringify(receipt, null, 2)}</pre>} />
    </PageNotice>}
    {status && draft === null && <>
      {status.rules.length === 0
        ? <PageEmptyState action={<Button disabled={!writable} onClick={() => edit(true)} size="sm" type="button">
          <Plus data-icon="inline-start" />{t("pluginPrewarmPolicy.add")}</Button>}
          description={t("pluginPrewarmPolicy.emptyHint")} icon={<Flame />} title={t("pluginPrewarmPolicy.disabled")} />
        : <PageItemList>
          {status.rules.map((rule) => <PageItem description={formatMessage(t("pluginPrewarmPolicy.ruleSummary"), { count: rule.targetIdle })}
            icon={<Flame />} key={rule.toolId} title={<>{nameOf(rule.pluginId)} · <code>{rule.toolId}</code></>} tone="success" />)}
        </PageItemList>}
      {/* A stale read already offers refresh in its notice, so this quiet control appears only for fresh data. */}
      {/* 陈旧读取已在提示中提供刷新，因此此低调控件仅在数据最新时显示。 */}
      <PageToolbar actions={fresh && <Button disabled={busy} onClick={() => void management.refresh()} size="sm" type="button" variant="ghost">
        <RefreshCw data-icon="inline-start" />{t("pluginRuntime.refresh")}</Button>}>
        {status.rules.length > 0 && <Button disabled={!writable} onClick={() => edit(false)} size="sm" type="button" variant="outline">{t("pluginPrewarmPolicy.edit")}</Button>}
      </PageToolbar>
    </>}
    {draft && <div className="plugin-prewarm-editor">
      {!currentDraft && <PageNotice tone="warning">{t("pluginPrewarmPolicy.draftStale")}</PageNotice>}
      {/* The fieldset locks only the rows; cancel stays reachable so a stale draft can always be discarded. */}
      {/* fieldset 仅锁定规则行；取消保持可用，使过期草稿始终可以丢弃。 */}
      <fieldset className="plugin-prewarm-editor-fields" disabled={!writable || !currentDraft}>
        <legend className="sr-only">{t("pluginPrewarmPolicy.replacement")}</legend>
        {draft.rows.length > 0 && <div className="plugin-prewarm-editor-head" aria-hidden="true">
          <span>{t("pluginPrewarmPolicy.plugin")}</span><span>{t("pluginPrewarmPolicy.tool")}</span><span>{t("pluginPrewarmPolicy.count")}</span><span />
        </div>}
        {draft.rows.map((row) => {
          const owner = props.plugins.find((plugin) => plugin.id === row.pluginId);
          // options keeps an unavailable configured package selectable instead of silently dropping it.
          // options 保留暂不可用的已配置包为可选项，而非静默丢弃。
          const options = luaPlugins.map((plugin) => ({ id: plugin.id, name: plugin.name }));
          if (owner === undefined && row.pluginId) options.push({ id: row.pluginId, name: row.pluginId });
          return <div className="plugin-prewarm-editor-row" key={row.id}>
            <label className="sr-only" htmlFor={`${prefix}-${row.id}-plugin`}>{t("pluginPrewarmPolicy.plugin")}</label>
            <Select onValueChange={(value) => update(row.id, { pluginId: value, toolId: "" })} value={row.pluginId}>
              <SelectTrigger id={`${prefix}-${row.id}-plugin`}><SelectValue placeholder={t("pluginPrewarmPolicy.plugin")} /></SelectTrigger>
              <SelectContent>{options.map((plugin) => <SelectItem key={plugin.id} value={plugin.id}>{plugin.name}</SelectItem>)}</SelectContent>
            </Select>
            <label className="sr-only" htmlFor={`${prefix}-${row.id}-tool`}>{t("pluginPrewarmPolicy.tool")}</label>
            <Input id={`${prefix}-${row.id}-tool`} list={`${prefix}-${row.id}-tools`} onChange={(event) => update(row.id, { toolId: event.target.value })}
              placeholder={t("pluginPrewarmPolicy.toolPlaceholder")} value={row.toolId} />
            <datalist id={`${prefix}-${row.id}-tools`}>{(owner?.tools ?? []).map((tool) => <option key={tool.id} value={tool.id} />)}</datalist>
            <label className="sr-only" htmlFor={`${prefix}-${row.id}-target`}>{t("pluginPrewarmPolicy.count")}</label>
            <Input id={`${prefix}-${row.id}-target`} inputMode="numeric" onChange={(event) => update(row.id, { target: event.target.value })} value={row.target} />
            <IconButton aria-label={t("pluginPrewarmPolicy.remove")} onClick={() => setDraft({ ...draft, rows: draft.rows.filter((candidate) => candidate.id !== row.id) })} type="button">
              <Trash2 size={15} /></IconButton>
          </div>;
        })}
        {draft.rows.length === 0 && <PageHint>{t("pluginPrewarmPolicy.emptyDraft")}</PageHint>}
        {rules === null && <PageHint role="status">{t("pluginPrewarmPolicy.invalid")}</PageHint>}
      </fieldset>
      <PageToolbar actions={<>
        <Button onClick={() => setDraft(null)} size="sm" type="button" variant="outline">{t("skills.plugins.cancel")}</Button>
        <Button disabled={!writable || !currentDraft || rules === null} onClick={save} size="sm" type="button">{t("pluginPrewarmPolicy.save")}</Button>
      </>}>
        <Button disabled={!writable || !currentDraft} onClick={() => setDraft({ ...draft, rows: [...draft.rows, blankRow()] })}
          size="sm" type="button" variant="ghost"><Plus data-icon="inline-start" />{t("pluginPrewarmPolicy.addRow")}</Button>
      </PageToolbar>
    </div>}
  </div>;
}
