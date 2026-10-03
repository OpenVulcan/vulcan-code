import * as React from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useI18n } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import type { PluginDiagnosticsSelection, PluginResourceDiagnostic } from "@/types/plugin-resource-diagnostics";
import { readPluginResourceDiagnostics, type PluginDiagnosticsObservation } from "@/service/plugin-resource-diagnostics-service";

// PluginResourceDiagnosticsProps binds read-only selection to the existing workspace conversation action.
// PluginResourceDiagnosticsProps 将只读选择绑定至既有工作台会话动作。
interface PluginResourceDiagnosticsProps {
  // selection is the exact original plugin or request selected by a card or retained request row.
  // selection 是卡片或保留请求行选中的精确原插件或请求。
  selection: PluginDiagnosticsSelection | null;
  // onSelect changes only diagnostic selection and never resumes or cancels service work.
  // onSelect 仅改变诊断选择，绝不恢复或取消服务工作。
  onSelect: (selection: PluginDiagnosticsSelection) => void;
  // conversations contains the authoritative workspace conversation identities.
  // conversations 包含权威工作台会话身份。
  conversations: WorkbenchSnapshot["conversations"];
  // onSelectPluginResourceOwner confirms the exact service-selected owner before switching to chat.
  // onSelectPluginResourceOwner 切换聊天前确认服务选中的精确所有者。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
}

// PluginResourceDiagnostics renders retained metadata for props.selection and returns readonly navigation controls.
// PluginResourceDiagnostics 为 props.selection 渲染保留元数据，返回只读导航控件。
export function PluginResourceDiagnostics(props: PluginResourceDiagnosticsProps) {
  // expanded follows explicit card/request navigation while preserving the selected original across collapse.
  // expanded 跟随明确卡片或请求导航，跨折叠保留选中原实例。
  const [expanded, setExpanded] = React.useState(false);
  // pluginDraft allows explicit reads for retained owners no longer present in the installed catalog.
  // pluginDraft 允许明确读取已不在安装目录中的保留所有者。
  const [pluginDraft, setPluginDraft] = React.useState("");
  // observation binds each report and failure to the exact selected object rather than merely a display name.
  // observation 将每份报告及失败绑定至精确选中对象，而非仅显示名称。
  const [observation, setObservation] = React.useState<{ selection: PluginDiagnosticsSelection; report: PluginDiagnosticsObservation | null; error: string; fresh: boolean } | null>(null);
  // busy marks this renderer's observation, without affecting any service-owned mutation.
  // busy 标记此渲染器观测，不影响任何服务拥有的变更。
  const [busy, setBusy] = React.useState(false);
  // version rejects stale refreshes, cancelled observations and previous selection replies.
  // version 拒绝过期刷新、取消观测及先前选择响应。
  const version = React.useRef(0);
  // alive suppresses asynchronous state writes after unmount.
  // alive 抑制卸载后的异步状态写入。
  const alive = React.useRef(true);
  // section provides a stable scroll and focus target for card/request navigation.
  // section 为卡片或请求导航提供稳定滚动及聚焦目标。
  const section = React.useRef<HTMLElement>(null);
  // contentId connects the accessible expansion control with its content.
  // contentId 连接无障碍展开控件及其内容。
  const contentId = React.useId();
  // t uses the existing locale catalog for every user-facing status.
  // t 为每个面向用户状态使用既有语言目录。
  const { t } = useI18n();

  // load reads one frozen selection; return completion without mutating any runtime resources.
  // load 读取单个固定选择；返回完成，不变更任何运行时资源。
  const load = React.useCallback(async (selection: PluginDiagnosticsSelection): Promise<void> => {
    // token owns this read independently from later selection or refresh changes.
    // token 独立于后续选择或刷新变更拥有此读取。
    const token = ++version.current;
    setBusy(true);
    setObservation((current) => current !== null && current.selection === selection ? { ...current, fresh: false } : null);
    try {
      // report contains safe original identity metadata only.
      // report 仅包含安全原身份元数据。
      const report = await readPluginResourceDiagnostics(selection);
      if (alive.current && version.current === token) setObservation({ selection, report, error: "", fresh: true });
    } catch (reason) {
      if (alive.current && version.current === token) setObservation((current) => ({ selection,
        report: current !== null && current.selection === selection ? current.report : null, error: String(reason), fresh: false }));
    } finally { if (alive.current && version.current === token) setBusy(false); }
  }, []);

  React.useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; version.current += 1; };
  }, []);
  React.useEffect(() => {
    version.current += 1; setBusy(false);
    if (props.selection === null) return;
    setExpanded(true); setPluginDraft(props.selection.pluginId);
    section.current?.focus();
    section.current?.scrollIntoView({ block: "nearest" });
    void load(props.selection);
    return () => { version.current += 1; };
  }, [props.selection, load]);

  // current excludes a previous selection immediately, even before its effect invalidates the old read.
  // current 立即排除先前选择，即使其效果尚未使旧读取失效。
  const current = observation !== null && observation.selection === props.selection ? observation : null;
  // resources remains absent when the original live drain cannot be observed, never an invented empty list.
  // resources 在原实时排空无法观测时保持缺失，绝不虚构空列表。
  const resources = current?.report?.resources;
  return <section ref={section} tabIndex={-1} className="space-y-3 rounded-md border p-3" aria-label={t("pluginDiagnostics.title")}>
    <Button type="button" variant="ghost" size="sm" aria-expanded={expanded} aria-controls={contentId}
      onClick={() => setExpanded(!expanded)}>{t("pluginDiagnostics.title")}</Button>
    <div id={contentId} hidden={!expanded} className="space-y-3 text-sm">
      <div className="flex flex-wrap gap-2">
        <Input aria-label={t("pluginUpdates.plugin")} value={pluginDraft} onChange={(event) => setPluginDraft(event.target.value)} />
        <Button type="button" size="sm" variant="outline" disabled={!pluginDraft.trim()}
          onClick={() => props.onSelect({ kind: "plugin", pluginId: pluginDraft.trim() })}>{t("pluginUpdates.read")}</Button>
        {props.selection !== null && <Button type="button" size="sm" variant="outline" disabled={busy}
          onClick={() => { if (props.selection !== null) void load(props.selection); }}>{t("pluginRuntime.refresh")}</Button>}
        {busy && <Button type="button" size="sm" variant="outline" onClick={() => { version.current += 1; setBusy(false); }}>{t("skills.plugins.cancel")}</Button>}
      </div>
      {props.selection !== null && <p className="break-all">{props.selection.pluginId} / {props.selection.kind}
        {props.selection.kind !== "plugin" && <> / {t("pluginUpdates.request")}: <code>{props.selection.requestId}</code></>}</p>}
      {busy && <p role="status">{t("ui.loading")}</p>}
      {current?.error && <p role="alert" className="whitespace-pre-wrap break-all">{current.error}</p>}
      {current !== null && !current.fresh && current.report !== null && <p role="status">{t("pluginUpdates.stale")}</p>}
      {current?.report?.update && <p className="break-all">serviceEpoch: {current.report.update.serviceEpoch} / predecessorGeneration: {current.report.update.predecessorGeneration} / drainingGeneration: {current.report.update.drainingGeneration}</p>}
      {current?.report?.uninstall && <p className="break-all">serviceEpoch: {current.report.uninstall.serviceEpoch} / pluginGeneration: {current.report.uninstall.pluginGeneration}</p>}
      {current?.report !== null && current?.report !== undefined && resources === null && <p role="status">{t("pluginDiagnostics.unavailable")}</p>}
      {resources && <>
        <p className="break-all">runtimeId: {resources.runtimeId} / serviceEpoch: {resources.serviceEpoch} / pluginGeneration: {resources.pluginGeneration === null ? "—" : resources.pluginGeneration}</p>
        <p>{t("pluginDiagnostics.nonAtomic")}</p>
        <p>{t("pluginDiagnostics.pinned")}: <strong>{resources.pinnedSessionCount}</strong></p>
        {resources.resources.length === 0 && <p role="status">{t("pluginDiagnostics.empty")}</p>}
        {resources.resources.map((resource) => <PluginResourceRow key={`${resource.resourceKind}:${resource.resourceId}:${resource.pluginGeneration}`} resource={resource}
          conversations={props.conversations} navigationEnabled={expanded} onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} />)}
      </>}
    </div>
  </section>;
}

// PluginResourceRow displays resource metadata and navigates only the exact available owner conversation.
// PluginResourceRow 展示 resource 元数据，仅导航至精确可用所有者会话。
// conversations validates owner visibility; onSelectPluginResourceOwner is the existing service chain; returns one readonly row.
// conversations 校验所有者可见性；onSelectPluginResourceOwner 是既有服务链；返回单个只读行。
function PluginResourceRow(props: { resource: PluginResourceDiagnostic; conversations: WorkbenchSnapshot["conversations"]; navigationEnabled: boolean; onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean> }) {
  // row keeps native and host metadata distinct throughout rendering.
  // row 在整个渲染过程区分原生及宿主元数据。
  const row = props.resource;
  // ownerExists uses only the original controller conversation ID, never coreSessionId or resourceId.
  // ownerExists 仅使用原控制器会话 ID，绝不使用 coreSessionId 或 resourceId。
  const ownerExists = row.ownerSessionId !== null && props.conversations.some((conversation) => conversation.id === row.ownerSessionId);
  // navigationError exposes missing originals and rejected selection without changing the diagnostic owner.
  // navigationError 暴露缺失原实例及选择拒绝，不改变诊断所有者。
  const [navigationError, setNavigationError] = React.useState("");
  // navigating disables duplicate renderer requests until the exact confirmation is received.
  // navigating 在收到精确确认前停用重复渲染器请求。
  const [navigating, setNavigating] = React.useState(false);
  // navigationVersion invalidates asynchronous replies after owner replacement, archival or disposal.
  // navigationVersion 在所有者替换、归档或销毁后使异步响应失效。
  const navigationVersion = React.useRef(0);
  // navigationBusy closes admission before React commits its disabled state.
  // navigationBusy 在 React 提交停用状态前关闭入场。
  const navigationBusy = React.useRef(false);
  // navigationAlive prevents stale renderer errors after unmount.
  // navigationAlive 阻止卸载后的过期渲染器错误。
  const navigationAlive = React.useRef(true);
  // navigationObservation cancels only the local confirmation decision, never the selection RPC or service work.
  // navigationObservation 仅取消本地确认决定，绝不取消选择 RPC 或服务工作。
  const navigationObservation = React.useRef<AbortController | null>(null);
  // t resolves labels without translating opaque identities or native codes.
  // t 解析标签，不翻译不透明身份或原生错误码。
  const { t } = useI18n();
  React.useLayoutEffect(() => {
    navigationAlive.current = true; navigationVersion.current += 1; navigationBusy.current = false;
    setNavigating(false);
    return () => { navigationAlive.current = false; navigationVersion.current += 1; navigationObservation.current?.abort(); };
  }, [row.ownerSessionId, ownerExists, props.navigationEnabled]);
  // navigateOwner requests this original owner and returns after observable service confirmation or refusal.
  // navigateOwner 请求此原所有者，在可观测服务确认或拒绝后返回。
  // No parameters; only an exact true response can authorize the workbench to leave diagnostics.
  // 无参数；仅精确真值响应可以授权工作台离开诊断。
  const navigateOwner = async (): Promise<void> => {
    // selectOwner is the exact confirmation callback provided by the real workbench.
    // selectOwner 是真实工作台提供的精确确认回调。
    const selectOwner = props.onSelectPluginResourceOwner;
    if (navigationBusy.current || !props.navigationEnabled || !ownerExists || row.ownerSessionId === null || selectOwner === undefined) return;
    // token binds renderer status to the requested original owner.
    // token 将渲染器状态绑定至请求原所有者。
    const token = ++navigationVersion.current;
    // observation belongs solely to this visible row and becomes aborted when its diagnostic owner is hidden.
    // observation 仅属于此可见行，在其诊断所有者被隐藏时变为已取消。
    const observation = new AbortController();
    navigationObservation.current = observation;
    navigationBusy.current = true; setNavigating(true); setNavigationError("");
    try {
      // selected is true only when the service has acknowledged this same owner identity.
      // selected 仅在服务确认此相同所有者身份时为真。
      const selected = await selectOwner(row.ownerSessionId, observation.signal);
      if (navigationAlive.current && navigationVersion.current === token && !selected) setNavigationError(t("pluginDiagnostics.missingOwner"));
    } catch (reason) {
      if (navigationAlive.current && navigationVersion.current === token) setNavigationError(`${t("pluginDiagnostics.missingOwner")} ${String(reason)}`);
    } finally {
      if (navigationAlive.current && navigationVersion.current === token) { navigationBusy.current = false; setNavigating(false); }
    }
  };
  return <article className="space-y-2 rounded-md border p-3 break-all">
    <p><strong>{row.resourceKind}</strong> / {row.resourceId} / pluginGeneration: {row.pluginGeneration}</p>
    <p>{t(row.isDrainBlocking ? "pluginDiagnostics.blocking" : "pluginDiagnostics.nonBlocking")}</p>
    <p>ownerSessionId: {row.ownerSessionId === null ? t("pluginDiagnostics.internal") : row.ownerSessionId}</p>
    {row.ownerSessionId !== null && (ownerExists && props.onSelectPluginResourceOwner !== undefined
      ? <Button type="button" size="sm" variant="outline" disabled={navigating} onClick={() => void navigateOwner()}>{t("pluginDiagnostics.locate")}</Button>
      : <p role="status">{t("pluginDiagnostics.missingOwner")}</p>)}
    {navigationError && <p role="alert" className="whitespace-pre-wrap">{navigationError}</p>}
    <p>originalPoolId: {row.originalPoolId === null ? "—" : row.originalPoolId} / coreSessionId: {row.coreSessionId === null ? "—" : row.coreSessionId} / corePhase: {row.corePhase === null ? "—" : row.corePhase}</p>
    <p>activeOperationId: {row.activeOperationId === null ? "—" : row.activeOperationId} / finalizationOperationId: {row.finalizationOperationId === null ? "—" : row.finalizationOperationId}</p>
    <p>preparing: {String(row.preparing)} / closing: {String(row.closing)} / retired: {String(row.retired)} / cleanupRecovery: {String(row.cleanupRecovery)} / pendingFinalizations: {String(row.pendingFinalizations)}</p>
    <p>queuedCalls: {row.queuedCalls === null ? "—" : row.queuedCalls} / borrowers: {row.borrowers === null ? "—" : row.borrowers}</p>
    {row.finalizationOperationIds.map((id) => <p key={id}>finalizationOperationId: {id}</p>)}
    {row.operations.map((operation) => <div key={operation.operationId}>
      <p>operationId: {operation.operationId} / phase: {operation.phase} / cancellationRequested: {String(operation.cancellationRequested)}</p>
      {operation.errors.map((error, index) => <p key={`${error.stage}:${index}`} role="status">{error.stage}: {error.code === null ? "—" : error.code}</p>)}
    </div>)}
    {row.errors.map((error, index) => <p key={`${error.stage}:${index}`} role="status">{error.stage}: {error.code === null ? "—" : error.code}</p>)}
  </article>;
}
