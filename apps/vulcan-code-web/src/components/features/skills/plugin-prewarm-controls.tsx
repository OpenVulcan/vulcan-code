import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useI18n, formatMessage } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import type { PluginPrewarmReceipt } from "@/types/plugin-prewarm";
import { parsePluginPrewarmSessionId } from "@/service/plugin-prewarm-contract";
import { usePluginRuntimeManagement } from "./use-plugin-runtime-management";
import { usePluginPrewarmManagement } from "./use-plugin-prewarm-management";

// PrewarmControlsProps supplies only active session choices and the initial navigation selection.
// PrewarmControlsProps 仅提供活动会话选项与初始导航选择。
type PrewarmControlsProps = Pick<WorkbenchSnapshot, "conversations" | "selectedConversationId">;

// PluginPrewarmControls renders an inert collapsed entry; props never grant plugin code management access.
// PluginPrewarmControls 渲染初始无动作的折叠入口；props 不向插件代码授予管理权限。
export function PluginPrewarmControls(props: PrewarmControlsProps) {
  // opened preserves uncertain receipt tracking while the user temporarily collapses the panel.
  // opened 在用户暂时折叠面板时保留不确定回执追踪。
  const [opened, setOpened] = React.useState(false);
  const [expanded, setExpanded] = React.useState(false);
  const id = React.useId();
  const { t } = useI18n();
  return <section className="space-y-3 rounded-md border p-3">
    <Button type="button" size="sm" variant="ghost" aria-expanded={expanded} aria-controls={id}
      onClick={() => { setOpened(true); setExpanded(!expanded); }}>{t("pluginPrewarm.title")}</Button>
    <div id={id} hidden={!expanded}>{opened && <PluginPrewarmManagement {...props} />}</div>
  </section>;
}

// PluginPrewarmManagement renders explicit session-scoped actions and returns their original receipt evidence.
// PluginPrewarmManagement 渲染明确的会话范围操作，并展示原回执证据。
function PluginPrewarmManagement(props: PrewarmControlsProps) {
  // parent is a read-only observation; receipt commands remain fenced by its native runtimeId.
  // parent 为只读观测；回执操作仍由其原生 runtimeId 约束。
  const parent = usePluginRuntimeManagement();
  const { t } = useI18n();
  const prefix = React.useId();
  // Initial navigation is used once; later navigation never retargets existing preparation or cleanup.
  // 初始导航仅使用一次；后续导航绝不改变既有准备或清理目标。
  const [ownerId, setOwnerId] = React.useState(() => props.conversations.find((session) => session.id === props.selectedConversationId)?.id ?? "");
  const [rememberedOwners, setRememberedOwners] = React.useState(() => new Map(props.conversations
    .filter((session) => session.id === props.selectedConversationId).map((session) => [session.id, session.title])));
  // selection freezes the tool generation instead of silently advancing a stale dropdown selection.
  // selection 冻结工具代次，避免陈旧下拉选择静默推进代次。
  const [selection, setSelection] = React.useState("");
  const [idle, setIdle] = React.useState("");
  // recoveryInput permits exact receipt lookup after a removed session disappears across page reloads.
  // recoveryInput 允许会话移除并跨页面重载消失后，仍精确查询回执。
  const [recoveryInput, setRecoveryInput] = React.useState("");
  const [recoveryError, setRecoveryError] = React.useState("");
  const runtime = parent.observation?.runtime;
  const owner = runtime && ownerId ? { runtimeId: runtime.runtimeId, ownerSessionId: ownerId } : null;
  const state = usePluginPrewarmManagement(owner);
  const observation = state.observation;
  const active = props.conversations.some((session) => session.id === ownerId);
  const owners = new Map(rememberedOwners);
  for (const session of props.conversations) owners.set(session.id, session.title);
  const canWrite = parent.fresh && !parent.busy && !state.busy && observation.fresh && !observation.pending;
  const selectedTool = observation.catalog?.tools.find((tool) => JSON.stringify([tool.toolId, tool.pluginGeneration]) === selection);
  // A nonempty decimal count is required; no automatic desired idle default is introduced.
  // 必须填写非空十进制数量；不引入自动期望空闲默认值。
  const targetIdle = /^(0|[1-9][0-9]*)$/.test(idle) ? Number(idle) : Number.NaN;
  const validTarget = Number.isSafeInteger(targetIdle) && selectedTool !== undefined && targetIdle <= selectedTool.maxResidentVms;

  // selectOwner freezes a visible session choice and retains its label for cleanup after removal.
  // selectOwner 冻结可见会话选择，并在移除后保留标签以供清理。
  const selectOwner = (id: string): void => {
    // title comes only from the displayed authority, not a guessed session or fallback workspace.
    // title 仅来自显示的权威数据，不猜测会话或回退工作区。
    const title = owners.get(id);
    if (title === undefined) return;
    setRememberedOwners((previous) => new Map(previous).set(id, title));
    setOwnerId(id); setSelection(""); setIdle("");
  };

  // selectRecoveryOwner selects a canonical user-supplied UUID for inspection without preparing or starting work.
  // selectRecoveryOwner 选择用户提供的规范 UUID 用于核查，不准备或启动工作。
  const selectRecoveryOwner = (): void => {
    try {
      // id is validated by the same wire contract and still needs host-side ownership checks on every RPC.
      // id 由同一线协议契约校验，每次 RPC 仍需宿主检查归属。
      const id = parsePluginPrewarmSessionId(recoveryInput);
      setRememberedOwners((previous) => new Map(previous).set(id, id));
      setOwnerId(id); setSelection(""); setIdle(""); setRecoveryError("");
    } catch (reason) { setRecoveryError(String(reason)); }
  };

  // prepare submits a bounded exact declaration and returns no business execution or implicit start.
  // prepare 提交有界精确声明，不执行业务或隐式启动。
  const prepare = (): void => {
    if (!canWrite || !active || !owner || !selectedTool || !validTarget) return;
    void state.execute({ kind: "prepare", request: { ...owner, pluginId: selectedTool.pluginId,
      toolId: selectedTool.toolId, pluginGeneration: selectedTool.pluginGeneration, targetIdle } });
  };

  // control operates only on the displayed original receipt; the host repeats all lifecycle checks.
  // control 仅操作显示的原回执；宿主再次校验全部生命周期条件。
  const control = (kind: "start" | "cancel" | "recover" | "forget", receipt: PluginPrewarmReceipt): void => {
    if (!canWrite || (kind === "start" && !active)) return;
    void state.execute({ kind, request: { runtimeId: receipt.runtimeId, ownerSessionId: receipt.ownerSessionId, invocationId: receipt.invocationId } });
  };

  return <div className="space-y-3 text-sm">
    <p className="text-muted-foreground">{t("pluginPrewarm.description")}</p>
    <Button type="button" size="sm" variant="outline" disabled={parent.busy || state.busy} onClick={() => void parent.refresh()}>{t("pluginRuntime.refresh")}</Button>
    {parent.error && <p role="alert" className="break-all text-destructive">{parent.error}</p>}
    {!parent.fresh && parent.observation && <p role="status">{t("pluginRuntime.staleRead")}</p>}
    {parent.fresh && !runtime && <p role="status">{t("pluginRuntime.noRuntime")}</p>}
    {runtime && <p className="break-all">{t("pluginRuntime.parent")}: {runtime.runtimeId}</p>}
    {state.history.map((entry) => <details key={JSON.stringify([entry.catalog!.runtimeId, entry.catalog!.ownerSessionId])} className="rounded-md border p-2">
      <summary>{t("pluginPrewarm.oldParent")} · {entry.catalog!.runtimeId} · {entry.catalog!.ownerSessionId}</summary>
      <pre className="overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(entry, null, 2)}</pre>
    </details>)}
    <div className="space-y-1">
      <label htmlFor={`${prefix}-owner`}>{t("pluginPrewarm.owner")}</label>
      <Select value={ownerId} onValueChange={selectOwner}>
        <SelectTrigger id={`${prefix}-owner`}><SelectValue placeholder={t("pluginPrewarm.owner")} /></SelectTrigger>
        <SelectContent>{[...owners].map(([id, title]) => <SelectItem key={id} value={id}>{title} · {id}</SelectItem>)}</SelectContent>
      </Select>
    </div>
    <div className="space-y-1">
      <label htmlFor={`${prefix}-recovery`}>{t("pluginPrewarm.recoveryOwner")}</label>
      <Input id={`${prefix}-recovery`} value={recoveryInput} onChange={(event) => setRecoveryInput(event.target.value)} />
      <Button type="button" size="sm" variant="outline" disabled={!recoveryInput} onClick={selectRecoveryOwner}>{t("pluginPrewarm.selectRecovery")}</Button>
      {recoveryError && <p role="alert" className="break-all text-destructive">{recoveryError}</p>}
    </div>
    {ownerId && !active && <p role="status">{t("pluginPrewarm.missingSession")}</p>}
    <Button type="button" size="sm" variant="outline" disabled={!owner || !parent.fresh || parent.busy || state.busy}
      onClick={() => void state.refresh()}>{t("pluginPrewarm.load")}</Button>
    {state.busy && <p role="status">{t("ui.loading")}</p>}
    {observation.error && <p role="alert" className="break-all text-destructive">{observation.error}</p>}
    {!observation.fresh && observation.catalog && <p role="status">{t("pluginRuntime.staleRead")}</p>}
    {observation.pending && <div className="space-y-2 rounded-md border p-2" role="status">
      <p>{t("pluginPrewarm.unknown")}</p>
      <pre className="overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(observation.pending, null, 2)}</pre>
      <p>{t("pluginPrewarm.ackWarning")}</p>
      <Button type="button" size="sm" variant="outline" disabled={!observation.fresh || state.busy} onClick={state.acknowledgeUncertainty}>{t("pluginRuntime.discardTracking")}</Button>
    </div>}
    {observation.catalog && <>
      <div className="space-y-1">
        <label htmlFor={`${prefix}-tool`}>{t("pluginPrewarm.tool")}</label>
        <Select value={selection} onValueChange={setSelection} disabled={!canWrite || !active}>
          <SelectTrigger id={`${prefix}-tool`}><SelectValue placeholder={t("pluginPrewarm.tool")} /></SelectTrigger>
          <SelectContent>{observation.catalog.tools.map((tool) => <SelectItem key={tool.toolId} value={JSON.stringify([tool.toolId, tool.pluginGeneration])}>
            {tool.pluginId} / {tool.toolId} · {tool.pluginGeneration}
          </SelectItem>)}</SelectContent>
        </Select>
      </div>
      <div className="space-y-1">
        <label htmlFor={`${prefix}-idle`}>{t("pluginPrewarm.target")}</label>
        <Input id={`${prefix}-idle`} inputMode="numeric" value={idle} onChange={(event) => setIdle(event.target.value)} disabled={!canWrite || !active} />
        {selectedTool && <p>{t("pluginRuntime.max")}: {selectedTool.maxResidentVms}</p>}
      </div>
      <Button type="button" size="sm" disabled={!canWrite || !active || !validTarget} onClick={prepare}>{t("pluginPrewarm.prepare")}</Button>
      {observation.catalog.targets.length === 0 && <p>{t("pluginPrewarm.empty")}</p>}
      {observation.catalog.targets.map((receipt) => <article key={receipt.invocationId} aria-label={receipt.invocationId} className="space-y-2 rounded-md border p-3">
        <p className="break-all">{receipt.pluginId} / {receipt.toolId} · {receipt.pluginGeneration}</p>
        <p className="break-all">{receipt.invocationId}</p>
        <p className="break-all">{receipt.workspaceRoot}</p>
        <p>{t("pluginPrewarm.target")}: {receipt.targetIdle}</p>
        <p>{t(receipt.retired ? "pluginPrewarm.retired" : receipt.started ? "pluginPrewarm.started" : "pluginPrewarm.prepared")}</p>
        {receipt.succeeded && <p>{t("pluginPrewarm.succeeded")}</p>}
        {receipt.requiresCleanupRecovery && <p role="status">{t("pluginPrewarm.needsRecovery")}</p>}
        {receipt.error !== null && <p role="alert" className="whitespace-pre-wrap break-all text-destructive">{receipt.error}</p>}
        {receipt.satisfied && <p>{formatMessage(t("pluginPrewarm.historical"), { ready: receipt.satisfied.ready })}</p>}
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" disabled={!canWrite || !active || receipt.started || receipt.retired || receipt.requiresCleanupRecovery} onClick={() => control("start", receipt)}>{t("pluginPrewarm.start")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canWrite || receipt.retired} onClick={() => control("cancel", receipt)}>{t("pluginPrewarm.cancel")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canWrite || !receipt.requiresCleanupRecovery} onClick={() => control("recover", receipt)}>{t("pluginPrewarm.recover")}</Button>
          <Button type="button" size="sm" variant="outline" disabled={!canWrite || !receipt.retired || receipt.requiresCleanupRecovery} onClick={() => control("forget", receipt)}>{t("pluginPrewarm.forget")}</Button>
        </div>
        <details><summary>{t("pluginPrewarm.evidence")}</summary><pre className="overflow-auto whitespace-pre-wrap break-all">{JSON.stringify(receipt, null, 2)}</pre></details>
      </article>)}
    </>}
  </div>;
}
