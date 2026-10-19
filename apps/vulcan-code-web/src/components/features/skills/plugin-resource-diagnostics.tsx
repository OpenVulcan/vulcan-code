import * as React from "react";
import { Activity, ArrowLeft, Cog, MessageSquare, RefreshCw } from "lucide-react";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure";
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
import type { WorkbenchSnapshot } from "@/types";
import type { PluginDiagnosticsSelection, PluginOperationPhase, PluginResourceDiagnostic, PluginResourceKind } from "@/types/plugin-resource-diagnostics";
import { readPluginResourceDiagnostics, type PluginDiagnosticsObservation } from "@/service/plugin-resource-diagnostics-service";
import { PageEmptyState, PageFacts, PageHint, PageItem, PageItemList, PageNotice, PageToolbar } from "@/components/page";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { TechnicalDetails } from "./plugin-maintenance-section";

// PluginResourceDiagnosticsProps binds read-only selection to the existing workspace conversation action.
// PluginResourceDiagnosticsProps 将只读选择绑定至既有工作台会话动作。
interface PluginResourceDiagnosticsProps {
  // selection is the exact original plugin or request selected by a card or retained request row.
  // selection 是卡片或保留请求行选中的精确原插件或请求。
  selection: PluginDiagnosticsSelection | null;
  // onSelect changes only diagnostic selection and never resumes or cancels service work.
  // onSelect 仅改变诊断选择，绝不恢复或取消服务工作。
  onSelect: (selection: PluginDiagnosticsSelection) => void;
  // conversations contains the authoritative workspace conversation identities and titles.
  // conversations 包含权威工作台会话身份与标题。
  conversations: WorkbenchSnapshot["conversations"];
  // onSelectPluginResourceOwner confirms the exact service-selected owner before switching to chat.
  // onSelectPluginResourceOwner 切换聊天前确认服务选中的精确所有者。
  onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean>;
  // pluginName resolves a display name; plugins absent from the catalog fall back to their ID.
  // pluginName 解析显示名称；不在目录中的插件回退为其 ID。
  pluginName?: (pluginId: string) => string;
}

// kindLabels names each resource registry as the user meets it in practice.
// kindLabels 以用户实际接触的方式命名各资源注册表。
const kindLabels: Record<PluginResourceKind, TranslationKey> = {
  pinned_session: "pluginDiagnostics.kind.pinnedSession", formal_call: "pluginDiagnostics.kind.formalCall",
  reusable_domain: "pluginDiagnostics.kind.reusableDomain", hook_runtime: "pluginDiagnostics.kind.hookRuntime",
  ui_action_runtime: "pluginDiagnostics.kind.uiActionRuntime",
};

// activePhases are operation phases that still occupy the resource; their labels describe the current activity.
// activePhases 是仍占用资源的操作阶段；其标签描述当前活动。
const activePhases: Partial<Record<PluginOperationPhase, TranslationKey>> = {
  queued: "pluginDiagnostics.state.queued", initializing: "pluginDiagnostics.state.initializing",
  running: "pluginDiagnostics.state.running", waiting_for_host: "pluginDiagnostics.state.waitingForHost",
  cleaning: "pluginDiagnostics.state.cleaning",
};

// describeState summarizes one resource in a single plain sentence, choosing the most relevant fact.
// describeState 以一句平实描述概括单个资源，选取最相关的事实。
function describeState(row: PluginResourceDiagnostic, t: (key: TranslationKey) => string): string {
  const active = row.operations.find((operation) => activePhases[operation.phase] !== undefined);
  if (active !== undefined) return t(activePhases[active.phase] as TranslationKey);
  if (row.retired) return t("pluginDiagnostics.state.retired");
  if (row.closing) return t("pluginDiagnostics.state.closing");
  if (row.preparing) return t("pluginDiagnostics.state.preparing");
  if (row.queuedCalls !== null && row.queuedCalls > 0) return formatMessage(t("pluginDiagnostics.state.waitingCalls"), { count: row.queuedCalls });
  return t("pluginDiagnostics.state.idle");
}

// hasErrors reports whether this resource or any of its operations recorded a failure.
// hasErrors 报告此资源或其任一操作是否记录了失败。
function hasErrors(row: PluginResourceDiagnostic): boolean {
  return row.errors.length > 0 || row.operations.some((operation) => operation.errors.length > 0);
}

// PluginResourceDiagnostics shows what is currently using a plugin, or what an update or uninstall is waiting for.
// PluginResourceDiagnostics 显示当前正在使用插件的内容，或更新、卸载正在等待的内容。
// It reads automatically for the selection and never changes any runtime resource.
// 它按选择自动读取，绝不改变任何运行时资源。
export function PluginResourceDiagnostics(props: PluginResourceDiagnosticsProps) {
  // observation binds each report and failure to the exact selected object rather than merely a display name.
  // observation 将每份报告及失败绑定至精确选中对象，而非仅显示名称。
  const [observation, setObservation] = React.useState<{ selection: PluginDiagnosticsSelection; report: PluginDiagnosticsObservation | null; error: string; fresh: boolean } | null>(null);
  // busy marks this renderer's observation, without affecting any service-owned mutation.
  // busy 标记此渲染器观测，不影响任何服务拥有的变更。
  const [busy, setBusy] = React.useState(false);
  // version rejects stale refreshes, cancelled observations and previous selection replies.
  // version 拒绝过期刷新、取消观测及先前选择响应。
  const version = React.useRef(0);
  const alive = React.useRef(true);
  // section is the focus target when a request row explicitly locates its blockers after mount.
  // section 是挂载后请求行明确定位其阻塞项时的聚焦目标。
  const section = React.useRef<HTMLElement>(null);
  // mounted suppresses focus stealing for the selection present at first render.
  // mounted 抑制首次渲染时已有选择的焦点抢夺。
  const mounted = React.useRef(false);
  const { t } = useI18n();
  const nameOf = props.pluginName ?? ((pluginId: string) => pluginId);

  // load reads one frozen selection; returns completion without mutating any runtime resources.
  // load 读取单个固定选择；返回完成，不变更任何运行时资源。
  const load = React.useCallback(async (selection: PluginDiagnosticsSelection): Promise<void> => {
    const token = ++version.current;
    setBusy(true);
    setObservation((current) => current !== null && current.selection === selection ? { ...current, fresh: false } : null);
    try {
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
    if (mounted.current) {
      section.current?.focus({ preventScroll: true });
      section.current?.scrollIntoView({ block: "nearest" });
    }
    mounted.current = true;
    void load(props.selection);
    return () => { version.current += 1; };
  }, [props.selection, load]);

  // current excludes a previous selection immediately, even before its effect invalidates the old read.
  // current 立即排除先前选择，即使其效果尚未使旧读取失效。
  const current = observation !== null && observation.selection === props.selection ? observation : null;
  // resources remains absent when the original live drain cannot be observed, never an invented empty list.
  // resources 在原实时排空无法观测时保持缺失，绝不虚构空列表。
  const resources = current?.report?.resources;
  const selection = props.selection;
  if (selection === null) return null;
  const name = nameOf(selection.pluginId);
  // rows lists blockers first; finished records are folded away because they need no attention.
  // rows 先列出阻塞项；已结束记录被收起，因为它们无需关注。
  const live = resources?.resources.filter((row) => row.isDrainBlocking || !row.retired) ?? [];
  const history = resources?.resources.filter((row) => !row.isDrainBlocking && row.retired) ?? [];
  live.sort((left, right) => Number(right.isDrainBlocking) - Number(left.isDrainBlocking));
  const owners = new Set(live.flatMap((row) => row.ownerSessionId === null ? [] : [row.ownerSessionId]));
  const blocking = live.filter((row) => row.isDrainBlocking).length;
  return <section aria-label={t("pluginDiagnostics.title")} className="plugin-usage" ref={section} tabIndex={-1}>
    <PageToolbar actions={<>
      {selection.kind !== "plugin" && <Button onClick={() => props.onSelect({ kind: "plugin", pluginId: selection.pluginId })} size="sm" type="button" variant="ghost">
        <ArrowLeft data-icon="inline-start" />{t("pluginDiagnostics.showPlugin")}</Button>}
      {busy
        ? <Button onClick={() => { version.current += 1; setBusy(false); }} size="sm" type="button" variant="outline">{t("skills.plugins.cancel")}</Button>
        : <Button onClick={() => void load(selection)} size="sm" type="button" variant="outline"><RefreshCw data-icon="inline-start" />{t("pluginRuntime.refresh")}</Button>}
    </>}>
      <p className="vk-page-row-label">{selection.kind === "plugin"
        ? formatMessage(t("pluginDiagnostics.subject.plugin"), { name })
        : formatMessage(t(selection.kind === "update" ? "pluginDiagnostics.subject.update" : "pluginDiagnostics.subject.uninstall"), { name })}</p>
    </PageToolbar>
    {busy && current === null && <StreamPlaceholder count={2} label={t("ui.loading")} shape="rows" />}
    {current?.error && <PageNotice tone="error"><span className="whitespace-pre-wrap break-all">{current.error}</span></PageNotice>}
    {current !== null && !current.fresh && current.report !== null && !busy && <PageNotice tone="info">{t("pluginUpdates.stale")}</PageNotice>}
    {current?.report !== null && current?.report !== undefined && resources === null && <PageNotice tone="warning">{t("pluginDiagnostics.unavailable")}</PageNotice>}
    {resources && <>
      <PageFacts layout="tiles" items={[
        { key: "conversations", label: t("pluginDiagnostics.stat.conversations"), value: owners.size },
        { key: "blocking", label: t("pluginDiagnostics.stat.blocking"), value: blocking, tone: blocking > 0 ? "attention" : undefined },
        { key: "pinned", label: t("pluginDiagnostics.pinned"), value: resources.pinnedSessionCount },
      ]} />
      {live.length === 0 && <PageEmptyState title={t("pluginDiagnostics.empty")} />}
      {live.length > 0 && <PageItemList>
        {live.map((resource) => <PluginResourceRow conversations={props.conversations} key={`${resource.resourceKind}:${resource.resourceId}:${resource.pluginGeneration}`}
          onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} resource={resource} />)}
      </PageItemList>}
      {history.length > 0 && <InlineDisclosure keepMounted label={formatMessage(t("pluginDiagnostics.history"), { count: history.length })}>
        <PageItemList>
          {history.map((resource) => <PluginResourceRow conversations={props.conversations} key={`${resource.resourceKind}:${resource.resourceId}:${resource.pluginGeneration}`}
            onSelectPluginResourceOwner={props.onSelectPluginResourceOwner} resource={resource} />)}
        </PageItemList>
      </InlineDisclosure>}
      <PageHint>{t("pluginDiagnostics.nonAtomic")}</PageHint>
    </>}
    <TechnicalDetails entries={[
      [t("pluginUpdates.plugin"), <code key="plugin">{selection.pluginId}</code>],
      ...(selection.kind !== "plugin" ? [[t("pluginUpdates.request"), <code key="request">{selection.requestId}</code>] as const] : []),
      ...(resources ? [["runtimeId", <code key="runtime">{resources.runtimeId}</code>] as const, ["serviceEpoch", <code key="epoch">{resources.serviceEpoch}</code>] as const,
        ["pluginGeneration", <code key="generation">{resources.pluginGeneration ?? "—"}</code>] as const] : []),
      ...(current?.report?.update ? [["predecessorGeneration", <code key="predecessor">{current.report.update.predecessorGeneration}</code>] as const,
        ["drainingGeneration", <code key="draining">{current.report.update.drainingGeneration}</code>] as const] : []),
      ...(current?.report?.uninstall ? [["reservedGeneration", <code key="reserved">{current.report.uninstall.pluginGeneration}</code>] as const] : []),
    ]} />
  </section>;
}

// PluginResourceRow describes one resource in plain words and navigates only to its exact available owner conversation.
// PluginResourceRow 以平实语言描述单个资源，仅导航至其精确可用所有者会话。
function PluginResourceRow(props: { resource: PluginResourceDiagnostic; conversations: WorkbenchSnapshot["conversations"]; onSelectPluginResourceOwner?: (conversationId: string, observation: AbortSignal) => Promise<boolean> }) {
  const row = props.resource;
  // owner uses only the original controller conversation ID, never coreSessionId or resourceId.
  // owner 仅使用原控制器会话 ID，绝不使用 coreSessionId 或 resourceId。
  const owner = row.ownerSessionId === null ? undefined : props.conversations.find((conversation) => conversation.id === row.ownerSessionId);
  const ownerExists = owner !== undefined;
  // navigationError exposes missing originals and rejected selection without changing the diagnostic owner.
  // navigationError 暴露缺失原实例及选择拒绝，不改变诊断所有者。
  const [navigationError, setNavigationError] = React.useState("");
  const [navigating, setNavigating] = React.useState(false);
  // navigationVersion invalidates asynchronous replies after owner replacement, archival or disposal.
  // navigationVersion 在所有者替换、归档或销毁后使异步响应失效。
  const navigationVersion = React.useRef(0);
  const navigationBusy = React.useRef(false);
  const navigationAlive = React.useRef(true);
  // navigationObservation cancels only the local confirmation decision, never the selection RPC or service work.
  // navigationObservation 仅取消本地确认决定，绝不取消选择 RPC 或服务工作。
  const navigationObservation = React.useRef<AbortController | null>(null);
  const { t } = useI18n();
  React.useLayoutEffect(() => {
    navigationAlive.current = true; navigationVersion.current += 1; navigationBusy.current = false;
    setNavigating(false);
    return () => { navigationAlive.current = false; navigationVersion.current += 1; navigationObservation.current?.abort(); };
  }, [row.ownerSessionId, ownerExists]);
  // navigateOwner requests this original owner; only an exact true response lets the workbench leave diagnostics.
  // navigateOwner 请求此原所有者；仅精确真值响应允许工作台离开诊断。
  const navigateOwner = async (): Promise<void> => {
    const selectOwner = props.onSelectPluginResourceOwner;
    if (navigationBusy.current || !ownerExists || row.ownerSessionId === null || selectOwner === undefined) return;
    const token = ++navigationVersion.current;
    const observation = new AbortController();
    navigationObservation.current = observation;
    navigationBusy.current = true; setNavigating(true); setNavigationError("");
    try {
      const selected = await selectOwner(row.ownerSessionId, observation.signal);
      if (navigationAlive.current && navigationVersion.current === token && !selected) setNavigationError(t("pluginDiagnostics.missingOwner"));
    } catch (reason) {
      if (navigationAlive.current && navigationVersion.current === token) setNavigationError(`${t("pluginDiagnostics.missingOwner")} ${String(reason)}`);
    } finally {
      if (navigationAlive.current && navigationVersion.current === token) { navigationBusy.current = false; setNavigating(false); }
    }
  };
  const ownerText = row.ownerSessionId === null ? t("pluginDiagnostics.internal") : owner !== undefined ? owner.title : t("pluginDiagnostics.missingOwner");
  const Icon = row.ownerSessionId === null ? Cog : row.resourceKind === "formal_call" ? Activity : MessageSquare;
  return <PageItem
    actions={row.ownerSessionId !== null && ownerExists && props.onSelectPluginResourceOwner !== undefined
      ? <Button disabled={navigating} onClick={() => void navigateOwner()} size="sm" type="button" variant="outline">{t("pluginDiagnostics.locate")}</Button>
      : undefined}
    description={<>
      <span role={row.ownerSessionId !== null && !ownerExists ? "status" : undefined}>{ownerText}</span>
      <span aria-hidden="true"> · </span>
      <span>{describeState(row, t)}</span>
    </>}
    icon={<Icon />}
    meta={<>
      {row.isDrainBlocking && <Badge variant="warning">{t("pluginDiagnostics.blocking")}</Badge>}
      {row.cleanupRecovery && <Badge variant="outline">{t("pluginDiagnostics.cleanupPending")}</Badge>}
      {hasErrors(row) && <Badge variant="destructive">{t("pluginDiagnostics.hadErrors")}</Badge>}
    </>}
    title={t(kindLabels[row.resourceKind])}
    tone={row.isDrainBlocking ? "attention" : "neutral"}
  >
      {navigationError && <PageNotice tone="error">{navigationError}</PageNotice>}
      <TechnicalDetails entries={[
        ["resourceId", <code key="resource">{row.resourceId}</code>],
        ["pluginGeneration", <code key="generation">{row.pluginGeneration}</code>],
        ["ownerSessionId", <code key="owner">{row.ownerSessionId ?? "—"}</code>],
        ["originalPoolId", <code key="pool">{row.originalPoolId ?? "—"}</code>],
        ["coreSessionId", <code key="core">{row.coreSessionId ?? "—"}</code>],
        ["corePhase", <code key="phase">{row.corePhase ?? "—"}</code>],
        ["activeOperationId", <code key="active">{row.activeOperationId ?? "—"}</code>],
        ["finalizationOperationId", <code key="final">{[row.finalizationOperationId, ...row.finalizationOperationIds].filter((id) => id !== null).join(", ") || "—"}</code>],
        ["queuedCalls / borrowers", <code key="counts">{row.queuedCalls ?? "—"} / {row.borrowers ?? "—"}</code>],
        ["flags", <code key="flags">{[row.preparing && "preparing", row.closing && "closing", row.retired && "retired", row.cleanupRecovery && "cleanupRecovery",
          row.pendingFinalizations && "pendingFinalizations"].filter(Boolean).join(", ") || "—"}</code>],
        ...row.operations.map((operation) => [`operation ${operation.operationId}`, <code key={operation.operationId}>{operation.phase}
          {operation.cancellationRequested ? " (cancellationRequested)" : ""}
          {operation.errors.map((error) => ` · ${error.stage}: ${error.code ?? "—"}`).join("")}</code>] as const),
        ...row.errors.map((error, index) => [`error ${index + 1}`, <code key={`error:${index}`}>{error.stage}: {error.code ?? "—"}</code>] as const),
      ]} />
  </PageItem>;
}
