import { useMcpText } from "./mcp-text"
import * as React from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import type { LogEntrySummary, McpServerSummary, RuntimeModuleSnapshot, StartupSnapshot } from "@/types"
import { loadMcpRuntimeSnapshot, loadMcpServerCapabilities, type McpServerInspection, type McpCapabilities, type McpConnectionStatus, type McpToolInspection } from "@/service/mcp-settings-service"
import { McpServerDialog, type McpSettingsActionProps } from "./mcp-server-dialog"
import { McpSchemaDetail } from "./mcp-schema-detail"
import "@/styles/mcp.css"

// connectionLabels distinguishes configured enablement from observed connectivity.
// connectionLabels 区分配置启用与已观测连接状态。
function connectionLabels(text: (zh: string, en: string) => string): Record<McpConnectionStatus, string> { return {
  disabled: text("禁用", "Disabled"), connecting: text("连接中", "Connecting"), connected: text("已连接", "Connected"),
  disconnected: text("连接断开", "Disconnected"), failed: text("失败", "Failed"), stopped: text("已停止", "Stopped"),
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
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()
  const statusLabels = connectionLabels(text)

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
      <div className="settings-panel-title-row"><h3>{text("MCP 服务器", "Servers")}</h3><Button size="sm" onClick={() => setCreating(true)}>{text("添加", "Add")}</Button></div>
      <Input aria-label={text("搜索 MCP", "Search MCP")} placeholder={text("搜索名称或标识", "Search name or ID")} value={search} onChange={e => setSearch(e.target.value)} />
      <div className="mcp-filter-row">
        <Select value={source} onValueChange={setSource}><SelectTrigger aria-label={text("来源", "Source")}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">{text("全部来源", "All sources")}</SelectItem><SelectItem value="user">{text("用户配置", "User")}</SelectItem><SelectItem value="plugin">{text("插件", "Plugin")}</SelectItem></SelectContent></Select>
        <Select value={status} onValueChange={setStatus}><SelectTrigger aria-label={text("连接状态", "Connection state")}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">{text("全部状态", "All states")}</SelectItem>{Object.entries(statusLabels).map(([key, label]) => <SelectItem key={key} value={key}>{label}</SelectItem>)}<SelectItem value="unknown">{text("未取得状态", "Unknown")}</SelectItem></SelectContent></Select>
      </div>
      <div className="mcp-server-list">{filtered.map(row => <button type="button" disabled={removing} className="mcp-server-row" data-active={selectedId === row.id} key={row.id} onClick={() => setSelectedId(row.id)}>
        <strong>{row.name}</strong><span>{row.transport} · {row.source === "plugin" ? text("插件", "Plugin") : text("用户", "User")}</span>
        <span data-status={error ? "unknown" : row.runtime?.state.status}>{row.runtime ? `${error ? text("上次状态：", "Previous state: ") : ""}${statusLabels[row.runtime.state.status]}` : row.config?.enabled === false ? text("禁用", "Disabled") : text("已启用 · 未取得连接状态", "Enabled · State unavailable")}</span>
        {row.runtime && <span>{row.runtime.tools.length} tools</span>}
        {row.runtime?.state.lastError && <span className="mcp-error">{row.runtime.state.lastError}</span>}
      </button>)}</div>
      {!filtered.length && <p>{rows.length ? text("没有匹配的服务器", "No matches") : text("添加服务器后，在这里查看连接与工具。", "Add a server to inspect its connection and tools.")}</p>}
      <Button disabled={refreshing} size="sm" variant="outline" onClick={() => setRefreshKey(key => key + 1)}>{refreshing ? text("正在刷新", "Refreshing") : text("刷新状态", "Refresh status")}</Button>
      {error && <p role="alert" className="mcp-error">{error}<br />{text("保留上次读取结果，当前状态未确认。", "Previous snapshot retained; current state unconfirmed.")}</p>}
    </aside>
    <section className="panel mcp-detail">
      {selected ? <>
        <header className="settings-panel-title-row"><div><h3>{selected.name}</h3><small>{selected.id}</small></div>{selected.config && <div className="mcp-filter-row">
          <Button size="sm" variant="outline" onClick={() => setEditing(selected.config ?? null)}>{text("编辑", "Edit")}</Button>
          <Button size="sm" variant="outline" disabled={removing} onClick={() => setConfirmDelete(value => !value)}>{text("删除", "Delete")}</Button>
        </div>}</header>
        {confirmDelete && selected.config && <div className="mcp-delete-confirm"><p>{text("删除此服务器配置并停止其用户连接？", "Remove this server configuration?")}</p><Button variant="destructive" disabled={removing} onClick={async () => {
          if (removalInFlight.current) return
          removalInFlight.current = true
          setRemoving(true); setActionError("")
          try { await props.actions.onRemoveMcpServer(selected.id); setConfirmDelete(false); setRefreshKey(key => key + 1) }
          catch (failure) { setActionError(String(failure)) }
          finally { removalInFlight.current = false; setRemoving(false) }
        }}>{text("确认删除", "Confirm removal")}</Button><Button variant="ghost" disabled={removing} onClick={() => setConfirmDelete(false)}>{text("取消", "Cancel")}</Button></div>}
        <nav className="mcp-tabs" aria-label={text("MCP 详情", "MCP details")}>{[["overview", text("概览", "Overview")], ["tools", text("工具", "Tools")], ["capabilities", "Prompts / Resources"], ["diagnostics", text("诊断", "Diagnostics")]].map(([key, label]) => <Button key={key} size="sm" variant={tab === key ? "secondary" : "ghost"} aria-pressed={tab === key} onClick={() => setTab(key)}>{label}</Button>)}</nav>
        {actionError && <p role="alert" className="mcp-error">{actionError}</p>}
        {tab === "overview" && <>
          <p>{selected.runtime ? `${error ? text("上次状态：", "Previous state: ") : ""}${statusLabels[selected.runtime.state.status]}` : text("运行时尚未返回此服务器的连接状态", "No runtime state available")}</p>
          {selected.config && <p>{text("配置：", "Configuration: ")}{selected.config.enabled ? text("启用", "Enabled") : text("禁用", "Disabled")}</p>}
          {selected.runtime && <dl className="mcp-overview">
            <dt>{text("端点", "Endpoint")}</dt><dd>{selected.runtime.state.endpoint}</dd>
            <dt>{text("来源", "Source")}</dt><dd>{selected.runtime.state.source} {selected.runtime.state.pluginId} {selected.runtime.state.generation !== null && `generation ${selected.runtime.state.generation}`}</dd>
            <dt>{text("协议", "Protocol")}</dt><dd>{selected.runtime.state.protocolVersion ?? "—"}</dd>
            <dt>{text("实现", "Implementation")}</dt><dd>{selected.runtime.state.serverProduct ?? "—"} {selected.runtime.state.serverVersion}</dd>
            <dt>{text("已发现", "Discovered")}</dt><dd>{selected.runtime.tools.length} tools · {selected.runtime.state.promptCount} prompts · {selected.runtime.state.resourceCount} resources</dd>
          </dl>}
          <p className="settings-muted-line">{text("工具发现不等于当前 AI 已获调用权限；实际可用工具由会话与策略筛选。", "Discovery does not grant active AI tool permission.")}</p>
        </>}
        {tab === "tools" && <McpToolCatalog runtime={selected.runtime} stale={Boolean(error)} />}
        {tab === "capabilities" && <McpOptionalCapabilities key={`${selected.id}:${selected.runtime?.inspectionGeneration ?? "unavailable"}`} serverId={selected.id} connected={selected.runtime?.state.status === "connected" && !error} />}
        {tab === "diagnostics" && <>
          <p>{text("仅显示当前 MCP 服务器的有界诊断。", "Bounded diagnostics for this MCP server only.")}</p>
          {selected.runtime ? <><p>{text("重试", "Retries:")}{selected.runtime.state.retryCount}</p>
            {selected.runtime.state.lastError && <p className="mcp-error">{selected.runtime.state.lastError}</p>}
            <details><summary>{text("传输日志", "Transport logs")} ({selected.runtime.state.recentLogs.length})</summary><pre>{selected.runtime.state.recentLogs.join("\n") || text("暂无日志", "No logs")}</pre></details>
          </> : <p>{text("暂无运行时诊断", "Runtime diagnostics unavailable")}</p>}
          {props.onOpenDiagnostics && <Button variant="outline" onClick={props.onOpenDiagnostics}>{text("打开运行诊断", "Open runtime diagnostics")}</Button>}
        </>}
      </> : <p>{text("选择 MCP 服务器查看详情", "Select an MCP server")}</p>}
    </section>
    <McpServerDialog actions={props.actions} open={creating || editing !== null} server={editing} onOpenChange={open => { if (!open) { setCreating(false); setEditing(null); setRefreshKey(key => key + 1) } }} />
  </div>
}

// McpToolCatalog separates read-only discovery from active session tool permissions.
// McpToolCatalog 区分只读工具发现与当前会话工具权限。
function McpToolCatalog(props: { runtime: McpServerInspection | undefined; stale: boolean }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

  // query filters the full received catalog without another runtime operation.
  // query 过滤已接收的完整目录，不触发其他运行时操作。
  const [query, setQuery] = React.useState("")
  if (!props.runtime) return <p>{text("工具目录尚未取得", "Tool catalog unavailable")}</p>
  // Search feedback and rendered rows use the same retained catalog scope.
  // 搜索反馈与渲染条目采用相同的保留目录范围。
  const visibleTools = props.runtime.tools.filter(tool => (tool.name + " " + tool.description).toLowerCase().includes(query.toLowerCase()))
  return <div><p>{text("已发现工具目录，不代表当前会话可调用。", "Discovered catalog; current session permission may differ.")}</p>
    {(props.stale || props.runtime.state.status !== "connected") && <p className="mcp-error">{text("连接未确认；下列为保留的最近有效目录。", "Connection unconfirmed; retained last valid catalog.")}</p>}
    <Input aria-label={text("搜索工具", "Search tools")} value={query} onChange={e => setQuery(e.target.value)} placeholder={text("搜索工具", "Search tools")} />
    {visibleTools.map(tool => <McpToolDetails key={tool.name} tool={tool} />)}
    {!visibleTools.length && <p role="status">{props.runtime.tools.length ? text("没有匹配的工具", "No matching tools") : text("尚未发现工具", "No tools discovered")}</p>}
  </div>
}

// McpToolDetails defers schema rendering until the user expands the exact tool.
// McpToolDetails 将 Schema 渲染推迟到用户展开精确工具时。
function McpToolDetails(props: { tool: McpToolInspection }) {
  // text and expanded control localized display and expensive schema mounting.
  // text 与 expanded 控制本地化展示及开销较大的 Schema 挂载。
  const text = useMcpText()
  const [expanded, setExpanded] = React.useState(false)
  return <details className="mcp-tool" onToggle={event => setExpanded(event.currentTarget.open)}>
    <summary>{props.tool.name}</summary>{expanded && <><p>{props.tool.description}</p>
      <McpSchemaDetail title={text("输入参数", "Input parameters")} schema={props.tool.inputSchema} />
      <McpSchemaDetail title={text("输出结果", "Output result")} schema={props.tool.outputSchema} />
    </>}
  </details>
}

// McpOptionalCapabilities reads metadata only after the user opens this detail tab.
// McpOptionalCapabilities 仅在用户打开此详情标签后读取元数据。
function McpOptionalCapabilities(props: { serverId: string; connected: boolean }) {
  // text follows the shared application locale.
  // text 遵循共享应用语言。
  const text = useMcpText()

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
  if (!props.connected) return <p>{text("服务器未连接；此页面不会自动建立连接。", "Server disconnected; this page does not connect it.")}</p>
  return <div><p>{text("仅列出元数据，不读取资源正文或执行工具。", "Metadata only; no resource contents or tool execution.")}</p>
    <Button size="sm" variant="outline" disabled={busy} onClick={() => setRefreshKey(key => key + 1)}>{busy ? text("正在读取", "Loading") : text("刷新元数据", "Refresh metadata")}</Button>
    {error && <p role="alert">{error}</p>}{result && <>
      <h4>Prompts ({result.prompts?.length ?? "—"})</h4>{result.promptError ? <p role="alert">{result.promptError}</p> : result.prompts?.map(prompt => <details className="mcp-tool" key={prompt.name}>
        <summary>{prompt.name}</summary>{prompt.description && <p>{prompt.description}</p>}
        {prompt.arguments?.map(argument => <p key={argument.name}><strong>{argument.name}</strong> · {argument.required ? text("必填", "Required") : text("可选", "Optional")} {argument.description}</p>)}
        <details><summary>{text("完整元数据", "Full metadata")}</summary><pre>{JSON.stringify(prompt, null, 2)}</pre></details>
      </details>)}
      <h4>Resources ({result.resources?.length ?? "—"})</h4>{result.resourceError ? <p role="alert">{result.resourceError}</p> : result.resources?.map(resource => <details className="mcp-tool" key={resource.uri}>
        <summary>{resource.name}</summary><code>{resource.uri}</code>{resource.description && <p>{resource.description}</p>}<p>{resource.mimeType} {resource.size !== undefined && `${resource.size} bytes`}</p>
        <details><summary>{text("完整元数据", "Full metadata")}</summary><pre>{JSON.stringify(resource, null, 2)}</pre></details>
      </details>)}
    </>}
  </div>
}
