import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import type { PluginAutomaticPrewarmRule } from "@/types/plugin-prewarm-policy";
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
  // runtimeId fences replacement native parents.
  // runtimeId 隔离替换的原生父级。
  runtimeId: string;
  // revision is the original effective policy observed when editing began.
  // revision 为开始编辑时观测到的原生效策略。
  revision: string;
  // rows retains full replacement intent, including an explicit empty collection.
  // rows 保留全量替换意图，包括明确空集合。
  rows: RuleDraft[];
}

// draftRules validates explicit positive safe counts and unique tool ownership; null means invalid input.
// draftRules 校验明确正安全整数及唯一工具归属；空值表示输入无效。
function draftRules(rows: RuleDraft[]): PluginAutomaticPrewarmRule[] | null {
  // seen and rules build only validated exact fields, leaving manifest limits to the host authority.
  // seen 与 rules 仅构建已校验精确字段，清单限制仍归宿主权威。
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

// PluginPrewarmPolicyControls renders an inert collapsed editor and retains uncertain writes across collapse.
// PluginPrewarmPolicyControls 渲染初始无动作的折叠编辑器，跨折叠保留不确定写入。
export function PluginPrewarmPolicyControls() {
  // opened retains the observer after first expansion without mounting it on every navigation render.
  // opened 在首次展开后保留观测者，不因导航渲染反复挂载。
  const [opened, setOpened] = React.useState(false);
  const [expanded, setExpanded] = React.useState(false);
  const id = React.useId();
  const { t } = useI18n();
  return <section className="space-y-3 rounded-md border p-3">
    <Button type="button" size="sm" variant="ghost" aria-expanded={expanded} aria-controls={id}
      onClick={() => { setOpened(true); setExpanded(!expanded); }}>{t("pluginPrewarmPolicy.title")}</Button>
    <div id={id} hidden={!expanded}>{opened && <PolicyManagement />}</div>
  </section>;
}

// PolicyManagement exposes full replacement and original transaction controls; it never initializes plugins.
// PolicyManagement 暴露全量替换及原事务控件；绝不初始化插件。
function PolicyManagement() {
  // management keeps server authority separate from this editable local draft.
  // management 将服务权威与本地可编辑草稿分离。
  const management = usePluginPrewarmPolicy();
  const { observation, busy } = management;
  const { status, fresh, pending, acknowledged, error } = observation;
  const { t } = useI18n();
  const prefix = React.useId();
  const [draft, setDraft] = React.useState<PolicyDraft | null>(null);
  // writable requires an atomic current read with no competing or uncertain transaction.
  // writable 要求原子当前读取，且没有竞争或未知事务。
  const writable = fresh && !busy && pending === null && status !== null && status.update === null && !status.capacityUpdateRetained;
  const currentDraft = draft !== null && status !== null && draft.runtimeId === status.runtimeId && draft.revision === status.revision;
  const rules = draft === null ? null : draftRules(draft.rows);
  const receipt = status?.update;

  // edit explicitly replaces the local draft from the last observed effective rules; returns nothing.
  // edit 明确以最后观测的生效规则替换本地草稿；无返回值。
  const edit = (): void => {
    if (!writable || !status) return;
    setDraft({ runtimeId: status.runtimeId, revision: status.revision,
      rows: status.rules.map((rule) => ({ id: crypto.randomUUID(), pluginId: rule.pluginId, toolId: rule.toolId, target: String(rule.targetIdle) })) });
  };
  // update edits one stable local row without changing the original predecessor; returns nothing.
  // update 编辑单个稳定本地行，不改变原前驱；无返回值。
  const update = (id: string, changes: Partial<Pick<RuleDraft, "pluginId" | "toolId" | "target">>): void => {
    setDraft((original) => original === null ? null : { ...original, rows: original.rows.map((row) => row.id === id ? { ...row, ...changes } : row) });
  };
  // save signs one caller UUID for the frozen draft; the hook blocks duplicate same-tick submissions.
  // save 为冻结草稿签发单个调用方 UUID；钩子阻止同一时刻重复提交。
  const save = (): void => {
    if (!writable || !currentDraft || !draft || rules === null) return;
    void management.execute({ kind: "begin", request: { updateId: crypto.randomUUID(), runtimeId: draft.runtimeId,
      expectedRevision: draft.revision, rules } });
  };

  return <div className="space-y-3 text-sm">
    <p>{t("pluginPrewarmPolicy.description")}</p>
    <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => void management.refresh()}>{t("pluginPrewarmPolicy.read")}</Button>
    {error && <p role="alert">{error}</p>}
    {status && <>
      <p>{t("pluginPrewarmPolicy.revision")}: <code>{status.runtimeId} / {status.revision}</code></p>
      <div>{t("pluginPrewarmPolicy.effective")}: {status.rules.length === 0 ? t("pluginPrewarmPolicy.disabled") : <ul>
        {status.rules.map((rule) => <li key={rule.toolId}>{rule.pluginId} / {rule.toolId}: {rule.targetIdle}</li>)}
      </ul>}</div>
      {!fresh && <p role="status">{t("pluginPrewarmPolicy.stale")}</p>}
      {status.capacityUpdateRetained && <p role="status">{t("pluginPrewarmPolicy.capacityBlocked")}</p>}
      <Button type="button" size="sm" variant="outline" disabled={!writable} onClick={edit}>{t("pluginPrewarmPolicy.edit")}</Button>
    </>}
    {draft && <fieldset className="space-y-3" disabled={!writable || !currentDraft}>
      <legend>{t("pluginPrewarmPolicy.replacement")}</legend>
      {!currentDraft && <p role="status">{t("pluginPrewarmPolicy.draftStale")}</p>}
      {draft.rows.map((row) => <div key={row.id} className="grid gap-2 rounded border p-2 sm:grid-cols-4">
        <label htmlFor={`${prefix}-${row.id}-plugin`}>{t("pluginPrewarmPolicy.plugin")}<Input id={`${prefix}-${row.id}-plugin`} value={row.pluginId} onChange={(event) => update(row.id, { pluginId: event.target.value })} /></label>
        <label htmlFor={`${prefix}-${row.id}-tool`}>{t("pluginPrewarmPolicy.tool")}<Input id={`${prefix}-${row.id}-tool`} value={row.toolId} onChange={(event) => update(row.id, { toolId: event.target.value })} /></label>
        <label htmlFor={`${prefix}-${row.id}-target`}>{t("pluginPrewarm.target")}<Input id={`${prefix}-${row.id}-target`} inputMode="numeric" value={row.target} onChange={(event) => update(row.id, { target: event.target.value })} /></label>
        <Button type="button" size="sm" variant="outline" onClick={() => setDraft({ ...draft, rows: draft.rows.filter((candidate) => candidate.id !== row.id) })}>{t("pluginPrewarmPolicy.remove")}</Button>
      </div>)}
      {rules === null && <p role="status">{t("pluginPrewarmPolicy.invalid")}</p>}
      {draft.rows.length === 0 && <p>{t("pluginPrewarmPolicy.emptyDraft")}</p>}
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" variant="outline" onClick={() => setDraft({ ...draft, rows: [...draft.rows, { id: crypto.randomUUID(), pluginId: "", toolId: "", target: "" }] })}>{t("pluginPrewarmPolicy.add")}</Button>
        <Button type="button" size="sm" disabled={rules === null} onClick={save}>{t("pluginPrewarmPolicy.save")}</Button>
      </div>
    </fieldset>}
    {pending && <div role="status" className="space-y-2">
      <p>{t("pluginPrewarmPolicy.unknown")}</p><pre className="overflow-auto">{JSON.stringify(pending, null, 2)}</pre>
      <Button type="button" size="sm" variant="outline" disabled={busy || !fresh} onClick={management.acknowledgeUncertainty}>{t("pluginPrewarmPolicy.acknowledge")}</Button>
    </div>}
    {receipt && <div className="space-y-2">
      <p>{t("pluginPrewarmPolicy.transaction")}: <code>{receipt.request.updateId}</code> / {receipt.phase}</p>
      {receipt.error && <p role="alert">{receipt.error}</p>}
      {receipt.persistenceError && <p role="alert">{receipt.persistenceError}</p>}
      <div className="flex flex-wrap gap-2">{(["resume", "cancel", "forget"] as const).map((kind) =>
        <Button key={kind} type="button" size="sm" variant="outline" disabled={busy || !fresh || pending !== null || !policyReceiptAllows(receipt, kind)}
          onClick={() => void management.execute({ kind, request: receipt.request })}>{t(`pluginPrewarmPolicy.${kind}`)}</Button>)}</div>
      <details><summary>{t("pluginPrewarm.evidence")}</summary><pre className="overflow-auto">{JSON.stringify(receipt, null, 2)}</pre></details>
    </div>}
    {!fresh && acknowledged && <details open><summary>{t("pluginPrewarmPolicy.acknowledged")}</summary><pre className="overflow-auto">{JSON.stringify(acknowledged, null, 2)}</pre></details>}
  </div>;
}
