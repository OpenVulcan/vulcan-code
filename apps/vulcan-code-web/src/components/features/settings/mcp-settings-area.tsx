import * as React from "react"
import { Button } from "@/components/ui/button"
import { DisclosureSection, InlineDisclosure } from "@/components/ui/disclosure"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import type { LogEntrySummary, McpServerSummary, RuntimeModuleSnapshot, StartupSnapshot } from "@/types"
import { loadMcpRuntimeSnapshot, loadMcpServerCapabilities, type McpServerInspection, type McpCapabilities, type McpConnectionStatus, type McpToolInspection } from "@/service/mcp-settings-service"
import { McpServerDialog, type McpSettingsActionProps } from "./mcp-server-dialog"
import { McpSchemaDetail } from "./mcp-schema-detail"
import "@/styles/mcp.css"
import { useI18n, formatMessage, type Translate } from "@/i18n"

// connectionLabels distinguishes configured enablement from observed connectivity.
// connectionLabels 区分配置启用与已观测连接状态。
function connectionLabels(t: Translate): Record<McpConnectionStatus, string> { return {
  disabled: t("settings.mcp.disabled"), connecting: t("settings.mcp.connecting"), connected: t("settings.mcp.connected"),
  disconnected: t("settings.mcp.disconnected"), failed: t("settings.mcp.failed"), stopped: t("settings.mcp.stopped"),
  stopping: t("settings.mcp.stopping"),
} }

// McpSettingsArea reads the live Agent catalog and scopes diagnostics to the selected server.
// McpSettingsArea 读取实时 Agent 目录，并将诊断限定为所选服务器。
export function McpSettingsArea(props: {
  // Existing settings callbacks and persisted user configurations.
  // 现有设置回调与持久化用户配置。
  actions: McpSettingsActionProps; servers: McpServerSummary[]
  // Legacy host props stay accepted while global diagnostics move to their own page.
  // 保留宿主旧属性接线，全局诊断移至独立页面。
  logs: LogEntrySummary[]; modules: RuntimeModuleSnapshot[]; startup: StartupSnapshot
  // Navigates to the existing application-wide diagnostics surface.
  // 导航至现有应用全局诊断页面。
  onOpenDiagnostics?: () => void
}) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const statusLabels = connectionLabels(t)

  // Catalog and request state are page-local and never rewrite persisted configuration.
  // 目录与请求状态属于页面本地，不回写持久化配置。
  const [catalog, setCatalog] = React.useState<McpServerInspection[]>([])
  const [error, setError] = React.useState("")
  // actionError survives background status refreshes until a new explicit action or selection.
  // actionError 在后台状态刷新时保留，直到新的显式动作或选择发生。
  const [actionError, setActionError] = React.useState("")
  const [refreshing, setRefreshing] = React.useState(false)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [search, setSearch] = React.useState("")
  const [source, setSource] = React.useState("all")
  const [status, setStatus] = React.useState("all")
  const [tab, setTab] = React.useState("overview")
  const [editing, setEditing] = React.useState<McpServerSummary | null>(null)
  const [creating, setCreating] = React.useState(false)
  const [removing, setRemoving] = React.useState(false)
  // removalInFlight prevents duplicate removal requests before React commits the disabled state.
  // removalInFlight 在 React 提交禁用状态前阻止重复删除请求。
  const removalInFlight = React.useRef(false)
  const [confirmDelete, setConfirmDelete] = React.useState(false)
  const [refreshKey, setRefreshKey] = React.useState(0)
  React.useEffect(() => {
    // active and inFlight suppress stale responses and overlapping polling.
    // active 与 inFlight 阻止过期响应及重叠轮询。
    let active = true
    let inFlight = false
    const refresh = async () => {
      if (inFlight || document.visibilityState === "hidden") return
      inFlight = true; setRefreshing(true)
      try { const result = await loadMcpRuntimeSnapshot(); if (active) { setCatalog(result); setError("") } }
      catch (failure) { if (active) setError(String(failure)) }
      finally { inFlight = false; if (active) setRefreshing(false) }
    }
    void refresh()
    const timer = window.setInterval(() => void refresh(), 5000)
    document.addEventListener("visibilitychange", refresh)
    return () => { active = false; window.clearInterval(timer); document.removeEventListener("visibilitychange", refresh) }
  }, [refreshKey])
  // rows joins exact persisted identities; absent runtime state remains explicitly unknown.
  // rows 按精确持久化标识合并；缺失的运行时状态显式保持未知。
  const rows = [
    ...props.servers.map(config => ({ id: config.id, name: config.name, transport: config.transport, source: "user", config, runtime: catalog.find(entry => entry.state.id === config.id) })),
    ...catalog.filter(entry => entry.state.source === "plugin").map(runtime => ({ id: runtime.state.id, name: runtime.state.name, transport: runtime.state.transport, source: "plugin", config: undefined, runtime })),
  ]
  const filtered = rows.filter(row => (source === "all" || row.source === source)
    && (status === "all" || (row.runtime ? row.runtime.state.status : row.config?.enabled === false ? "disabled" : "unknown") === status)
    && (row.name + " " + row.id).toLowerCase().includes(search.toLowerCase()))
  const selected = rows.find(row => row.id === selectedId)
  React.useEffect(() => {
    if (!rows.some(row => row.id === selectedId)) setSelectedId(rows[0]?.id ?? null)
  }, [props.servers, catalog, selectedId])
  React.useEffect(() => { setTab("overview"); setConfirmDelete(false); setActionError("") }, [selectedId])
  return <div className="mcp-workspace">
    <aside className="panel mcp-directory">
      <div className="settings-panel-title-row"><h3>{t("settings.mcp.servers")}</h3><Button size="sm" onClick={() => setCreating(true)}>{t("settings.mcp.add")}</Button></div>
      <Input aria-label={t("settings.mcp.searchMcp")} placeholder={t("settings.mcp.searchNameOrId")} value={search} onChange={e => setSearch(e.target.value)} />
      <div className="mcp-filter-row">
        <Select value={source} onValueChange={setSource}><SelectTrigger aria-label={t("settings.mcp.source")}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">{t("settings.mcp.allSources")}</SelectItem><SelectItem value="user">{t("settings.mcp.user")}</SelectItem><SelectItem value="plugin">{t("settings.mcp.plugin")}</SelectItem></SelectContent></Select>
        <Select value={status} onValueChange={setStatus}><SelectTrigger aria-label={t("settings.mcp.connectionState")}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">{t("settings.mcp.allStates")}</SelectItem>{Object.entries(statusLabels).map(([key, label]) => <SelectItem key={key} value={key}>{label}</SelectItem>)}<SelectItem value="unknown">{t("settings.mcp.unknown")}</SelectItem></SelectContent></Select>
      </div>
      <div className="mcp-server-list">{filtered.map(row => <button type="button" disabled={removing} className="mcp-server-row" data-active={selectedId === row.id} key={row.id} onClick={() => setSelectedId(row.id)}>
        <strong>{row.name}</strong><span>{row.transport} · {row.source === "plugin" ? t("settings.mcp.plugin") : t("settings.mcp.user2")}</span>
        <span data-status={error ? "unknown" : row.runtime?.state.status}>{row.runtime ? `${error ? t("settings.mcp.previousState") : ""}${statusLabels[row.runtime.state.status]}` : row.config?.enabled === false ? t("settings.mcp.disabled") : t("settings.mcp.enabledStateUnavailable")}</span>
        {row.runtime && <span>{formatMessage(t("settings.mcp.tools"), { length: row.runtime.tools.length })}</span>}
        {row.runtime?.state.lastError && <span className="mcp-error">{row.runtime.state.lastError}</span>}
      </button>)}</div>
      {!filtered.length && <p>{rows.length ? t("settings.mcp.noMatches") : t("settings.mcp.addAServerTo")}</p>}
      <Button disabled={refreshing} size="sm" variant="outline" onClick={() => setRefreshKey(key => key + 1)}>{refreshing ? t("settings.mcp.refreshing") : t("settings.mcp.refreshStatus")}</Button>
      {error && <p role="alert" className="mcp-error">{error}<br />{t("settings.mcp.previousSnapshotRetainedCurrent")}</p>}
    </aside>
    <section className="panel mcp-detail">
      {selected ? <>
        <header className="settings-panel-title-row"><div><h3>{selected.name}</h3><small>{selected.id}</small></div>{selected.config && <div className="mcp-filter-row">
          <Button size="sm" variant="outline" onClick={() => setEditing(selected.config ?? null)}>{t("settings.mcp.edit")}</Button>
          <Button size="sm" variant="outline" disabled={removing} onClick={() => setConfirmDelete(value => !value)}>{t("settings.mcp.delete")}</Button>
        </div>}</header>
        {confirmDelete && selected.config && <div className="mcp-delete-confirm"><p>{t("settings.mcp.removeThisServerConfiguration")}</p><Button variant="destructive" disabled={removing} onClick={async () => {
          if (removalInFlight.current) return
          removalInFlight.current = true
          setRemoving(true); setActionError("")
          try { await props.actions.onRemoveMcpServer(selected.id); setConfirmDelete(false); setRefreshKey(key => key + 1) }
          catch (failure) { setActionError(String(failure)) }
          finally { removalInFlight.current = false; setRemoving(false) }
        }}>{t("settings.mcp.confirmRemoval")}</Button><Button variant="ghost" disabled={removing} onClick={() => setConfirmDelete(false)}>{t("settings.mcp.cancel")}</Button></div>}
        <nav className="mcp-tabs" aria-label={t("settings.mcp.mcpDetails")}>{[["overview", t("settings.mcp.overview")], ["tools", t("settings.mcp.tools2")], ["capabilities", t("settings.mcp.promptsResources")], ["diagnostics", t("settings.mcp.diagnostics")]].map(([key, label]) => <Button key={key} size="sm" variant={tab === key ? "secondary" : "ghost"} aria-pressed={tab === key} onClick={() => setTab(key)}>{label}</Button>)}</nav>
        {actionError && <p role="alert" className="mcp-error">{actionError}</p>}
        {tab === "overview" && <>
          <p>{selected.runtime ? `${error ? t("settings.mcp.previousState") : ""}${statusLabels[selected.runtime.state.status]}` : t("settings.mcp.noConnectionStatusYet")}</p>
          {selected.config && !selected.config.enabled && <p>{t("settings.mcp.thisServerIsDisabled")}</p>}
          {selected.runtime && <dl className="mcp-overview">
            <dt>{t("settings.mcp.address")}</dt><dd>{selected.runtime.state.endpoint}</dd>
            <dt>{t("settings.mcp.source")}</dt><dd>{selected.runtime.state.source === "plugin" ? formatMessage(t("settings.mcp.plugin2"), { pluginId: selected.runtime.state.pluginId ?? "" }) : t("settings.mcp.addedByYou")}{selected.runtime.state.generation !== null && formatMessage(t("settings.mcp.rev"), { generation: selected.runtime.state.generation })}</dd>
            <dt>{t("settings.mcp.protocolVersion")}</dt><dd>{selected.runtime.state.protocolVersion ?? "—"}</dd>
            <dt>{t("settings.mcp.server")}</dt><dd>{selected.runtime.state.serverProduct ?? "—"} {selected.runtime.state.serverVersion}</dd>
            <dt>{t("settings.mcp.provides")}</dt><dd>{formatMessage(t("settings.mcp.toolsPromptsResources"), { length: selected.runtime.tools.length, promptCount: selected.runtime.state.promptCount, resourceCount: selected.runtime.state.resourceCount })}</dd>
          </dl>}
          <p className="settings-muted-line">{t("settings.mcp.thisListsEveryTool")}</p>
        </>}
        {tab === "tools" && <McpToolCatalog runtime={selected.runtime} stale={Boolean(error)} />}
        {tab === "capabilities" && <McpOptionalCapabilities key={`${selected.id}:${selected.runtime?.inspectionGeneration ?? "unavailable"}`} serverId={selected.id} connected={selected.runtime?.state.status === "connected" && !error} />}
        {tab === "diagnostics" && <>
          <p>{t("settings.mcp.recentConnectionActivityFor")}</p>
          {selected.runtime ? <><p>{t("settings.mcp.retries")}{selected.runtime.state.retryCount}</p>
            {selected.runtime.state.lastError && <p className="mcp-error">{selected.runtime.state.lastError}</p>}
            <section className="mcp-logs"><h4>{t("settings.mcp.recentLogs")} ({selected.runtime.state.recentLogs.length})</h4><pre>{selected.runtime.state.recentLogs.join("\n") || t("settings.mcp.noLogs")}</pre></section>
          </> : <p>{t("settings.mcp.noDiagnosticsYet")}</p>}
          {props.onOpenDiagnostics && <Button variant="outline" onClick={props.onOpenDiagnostics}>{t("settings.mcp.openRuntimeDiagnostics")}</Button>}
        </>}
      </> : <p>{t("settings.mcp.selectAnMcpServer")}</p>}
    </section>
    <McpServerDialog actions={props.actions} open={creating || editing !== null} server={editing} onOpenChange={open => { if (!open) { setCreating(false); setEditing(null); setRefreshKey(key => key + 1) } }} />
  </div>
}

// McpToolCatalog separates read-only discovery from active session tool permissions.
// McpToolCatalog 区分只读工具发现与当前会话工具权限。
function McpToolCatalog(props: { runtime: McpServerInspection | undefined; stale: boolean }) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。

  // query filters the full received catalog without another runtime operation.
  // query 过滤已接收的完整目录，不触发其他运行时操作。
  const [query, setQuery] = React.useState("")
  if (!props.runtime) return <p>{t("settings.mcp.toolListNotAvailable")}</p>
  // Search feedback and rendered rows use the same retained catalog scope.
  // 搜索反馈与渲染条目采用相同的保留目录范围。
  const visibleTools = props.runtime.tools.filter(tool => (tool.name + " " + tool.description).toLowerCase().includes(query.toLowerCase()))
  return <div><p>{t("settings.mcp.toolsOfferedByThe")}</p>
    {(props.stale || props.runtime.state.status !== "connected") && <p className="mcp-error">{t("settings.mcp.canTReachThe")}</p>}
    <Input aria-label={t("settings.mcp.searchTools")} value={query} onChange={e => setQuery(e.target.value)} placeholder={t("settings.mcp.searchTools")} />
    {visibleTools.map(tool => <McpToolDetails key={tool.name} tool={tool} />)}
    {!visibleTools.length && <p role="status">{props.runtime.tools.length ? t("settings.mcp.noMatchingTools") : t("settings.mcp.noToolsDiscovered")}</p>}
  </div>
}

// McpToolDetails defers schema rendering until the user expands the exact tool.
// McpToolDetails 将 Schema 渲染推迟到用户展开精确工具时。
function McpToolDetails(props: { tool: McpToolInspection }) {
  const { t } = useI18n();
  // text and expanded control localized display and expensive schema mounting.
  // text 与 expanded 控制本地化展示及开销较大的 Schema 挂载。
  const [expanded, setExpanded] = React.useState(false)
  return <DisclosureSection className="mcp-tool" description={props.tool.description ? <span className="line-clamp-1">{props.tool.description}</span> : undefined} onOpenChange={setExpanded} title={props.tool.name}>
    {expanded && <><p>{props.tool.description}</p>
      <McpSchemaDetail title={t("settings.mcp.inputParameters")} schema={props.tool.inputSchema} />
      <McpSchemaDetail title={t("settings.mcp.outputResult")} schema={props.tool.outputSchema} />
    </>}
  </DisclosureSection>
}

// McpOptionalCapabilities reads metadata only after the user opens this detail tab.
// McpOptionalCapabilities 仅在用户打开此详情标签后读取元数据。
function McpOptionalCapabilities(props: { serverId: string; connected: boolean }) {
  const { t } = useI18n();
  // text follows the shared application locale.
  // text 遵循共享应用语言。

  // Metadata and errors are scoped to the selected server and discarded when unmounted.
  // 元数据与错误限定为所选服务器，卸载时丢弃。
  const [result, setResult] = React.useState<McpCapabilities | null>(null)
  const [error, setError] = React.useState("")
  const [busy, setBusy] = React.useState(false)
  const [refreshKey, setRefreshKey] = React.useState(0)
  React.useEffect(() => {
    let active = true
    setResult(null); setError("")
    if (!props.connected) return
    setBusy(true)
    void loadMcpServerCapabilities(props.serverId).then(value => { if (active) setResult(value) }).catch(failure => { if (active) setError(String(failure)) }).finally(() => { if (active) setBusy(false) })
    return () => { active = false }
  }, [props.serverId, props.connected, refreshKey])
  if (!props.connected) return <p>{t("settings.mcp.serverNotConnectedContent")}</p>
  return <div><p>{t("settings.mcp.namesAndDescriptionsOnly")}</p>
    <Button size="sm" variant="outline" disabled={busy} onClick={() => setRefreshKey(key => key + 1)}>{busy ? t("settings.mcp.loading") : t("settings.mcp.refresh")}</Button>
    {error && <p role="alert">{error}</p>}{result && <>
      <h4>{t("settings.mcp.prompts")} ({result.prompts?.length ?? "—"})</h4>{result.promptError ? <p role="alert">{result.promptError}</p> : result.prompts?.map(prompt => <DisclosureSection className="mcp-tool" description={prompt.description ? <span className="line-clamp-1">{prompt.description}</span> : undefined} key={prompt.name} title={prompt.name}>
        {prompt.description && <p>{prompt.description}</p>}
        {prompt.arguments?.map(argument => <p key={argument.name}><strong>{argument.name}</strong> · {argument.required ? t("settings.mcp.required") : t("settings.mcp.optional")} {argument.description}</p>)}
        <InlineDisclosure label={t("settings.mcp.viewRawData")} openLabel={t("settings.mcp.hideRawData")}><pre>{JSON.stringify(prompt, null, 2)}</pre></InlineDisclosure>
      </DisclosureSection>)}
      <h4>{t("settings.mcp.resources")} ({result.resources?.length ?? "—"})</h4>{result.resourceError ? <p role="alert">{result.resourceError}</p> : result.resources?.map(resource => <DisclosureSection className="mcp-tool" description={<span className="line-clamp-1 font-mono">{resource.uri}</span>} key={resource.uri} title={resource.name}>
        <code>{resource.uri}</code>{resource.description && <p>{resource.description}</p>}<p>{resource.mimeType} {resource.size !== undefined && `${resource.size} bytes`}</p>
        <InlineDisclosure label={t("settings.mcp.viewRawData")} openLabel={t("settings.mcp.hideRawData")}><pre>{JSON.stringify(resource, null, 2)}</pre></InlineDisclosure>
      </DisclosureSection>)}
    </>}
  </div>
}
