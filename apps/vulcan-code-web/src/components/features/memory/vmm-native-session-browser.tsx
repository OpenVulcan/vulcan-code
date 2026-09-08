import * as React from "react"
import { VmmNativeRemoval } from "./vmm-native-removal"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmMemorySnapshot, VmmNativeProjectCatalogSummary, VmmNativeSessionListPage, VmmNativeSessionListQuery } from "@/types"
import { nativeProjectOwnerId, VmmNativeMemoryRecords } from "./vmm-native-memory-records"
import { NativeSessionStatus, VmmNativeSessionArchive } from "./vmm-native-session-archive"

// BrowserProps retains the native management transport and current host-project context.
// BrowserProps 保留原生管理传输及当前宿主项目上下文。
interface BrowserProps {
  // Occupancy and snapshot come from the existing memory controller.
  // 占用状态与快照来自既有记忆控制器。
  busy: boolean
  snapshot: Pick<VmmMemorySnapshot, "selectedProjectId" | "selectedConversationId">
  // Executes an exact typed native action; null is failure, never an empty successful page.
  // 执行准确的类型化原生动作；空返回表示失败，绝不是成功的空页。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
}

// RecordSelection uses stable identities; null session explicitly selects project-wide provenance.
// RecordSelection 使用稳定身份；空会话明确选择项目级来源。
interface RecordSelection {
  projectId: number
  sessionSourceId: string | null
}

// Lifecycle and extraction selectors map only the closed service enums to existing labels.
// 生命周期与提取选择器仅将封闭的服务枚举映射至既有文案。
const lifecycleOptions: Array<[VmmNativeSessionListQuery["status"], TranslationKey]> = [
  ["all", "memory.sessionList.allStates"], ["active", "memory.sessionArchive.active"],
  ["archived", "memory.sessionArchive.archived"], ["recycled", "memory.sessionArchive.recycled"],
  ["forgotten", "memory.sessionArchive.forgotten"], ["unregistered", "memory.sessionArchive.unregistered"],
]
const extractionOptions: Array<[VmmNativeSessionListQuery["extractionStatus"], TranslationKey]> = [
  ["all", "memory.sessionList.allExtraction"], ["pending", "memory.nativeRecords.pending"],
  ["completed", "memory.nativeRecords.completed"], ["processed_unknown", "memory.nativeRecords.processedUnknown"],
  ["analysis_bypassed", "memory.nativeRecords.bypassed"], ["corrupted", "memory.nativeRecords.corrupted"],
]

// Builds a fresh bounded query for a selected stable project; returns no implicit session filter.
// 为选定稳定项目构建全新有界查询；不隐式添加会话筛选。
export function nativeSessionInitialQuery(projectId: number): VmmNativeSessionListQuery {
  return { vmmProjectId: projectId, status: "all", extractionStatus: "all", sort: "last_activity_desc", search: "", lastActivityFromUnixMs: null, lastActivityBeforeUnixMs: null, offset: 0, limit: 20, expectedStorageVersion: null }
}

// Converts a local calendar day to an inclusive midnight or exclusive next midnight, honoring DST.
// 将本地日历日转换为包含的零点或不包含的次日零点，遵循夏令时。
// Empty input returns null; malformed calendar days throw instead of silently normalizing dates.
// 空输入返回空值；非法日历日期抛错，不静默规范化日期。
export function nativeSessionDateBound(value: string, exclusive: boolean): number | null {
  if (value === "") return null
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) throw new Error("Invalid calendar date")
  const [year, month, day] = value.split("-").map(Number)
  const date = new Date(0)
  date.setFullYear(year, month - 1, day)
  date.setHours(0, 0, 0, 0)
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day || date.getTime() < 0) throw new Error("Invalid calendar date")
  if (exclusive) date.setDate(date.getDate() + 1)
  return date.getTime()
}

// Switches from lightweight browsing to an explicitly selected provenance view without eager bodies.
// 从轻量浏览切换到明确选定的来源视图，不提前加载正文。
export function VmmNativeSessionBrowser(props: BrowserProps) {
  const { t } = useI18n()
  const [selection, setSelection] = React.useState<RecordSelection | null>(null)
  // revision tracks accepted source mutations that make the retained summary page stale.
  // revision 跟踪会使保留摘要页过时的已接受来源修改。
  const [revision, setRevision] = React.useState(0)
  // detailRevision invalidates open records after a mutation initiated from the retained list.
  // detailRevision 在保留列表发起修改后，使已打开记录失效。
  const [detailRevision, setDetailRevision] = React.useState(0)
  // runListAction preserves list refresh ownership while reloading any open detail after writes.
  // runListAction 保留列表自身刷新职责，并在写入后重新加载已打开详情。
  const runListAction = React.useCallback(async (action: string, payload?: Record<string, unknown>) => {
    const reply = await props.onRunAction(action, payload)
    if (reply && ["native_session_archive", "native_removal"].includes(reply.data.kind)) setDetailRevision((current) => current + 1)
    return reply
  }, [props.onRunAction])
  // runDetailAction returns the original reply and invalidates only confirmed mutation kinds.
  // runDetailAction 返回原始响应，仅对确认的修改类型失效列表。
  const runDetailAction = React.useCallback(async (action: string, payload?: Record<string, unknown>) => {
    const reply = await props.onRunAction(action, payload)
    if (reply && ["native_session_archive", "native_session_restart", "native_removal"].includes(reply.data.kind)) setRevision((current) => current + 1)
    return reply
  }, [props.onRunAction])
  return <div className={selection ? "vmm-session-workspace has-selection" : "vmm-session-workspace"}>
    <div className="vmm-session-browser"><NativeSessionList {...props} onRunAction={runListAction} revision={revision} onOpen={setSelection} onScopeChange={() => setSelection(null)} selection={selection} /></div>
    {selection ? <div className="vmm-session-detail space-y-3">
      <Button variant="outline" disabled={props.busy} onClick={() => setSelection(null)}>{t("memory.sessionList.back")}</Button>
      <VmmNativeMemoryRecords key={`${selection.projectId}:${selection.sessionSourceId}:${detailRevision}`} {...props} onRunAction={runDetailAction} initialSelection={selection} />
    </div> : null}
  </div>
}

// Loads one summary page at a time, clears hidden selections and delegates mutations to preview/confirm.
// 每次加载一页摘要，清除不可见选择，并将修改委托给预览确认流程。
function NativeSessionList({ busy, snapshot, onRunAction, onOpen, onScopeChange, selection, revision }: BrowserProps & { revision: number; onOpen: (selection: RecordSelection) => void; onScopeChange: () => void; selection: RecordSelection | null }) {
  const { t } = useI18n()
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [query, setQuery] = React.useState<VmmNativeSessionListQuery | null>(null)
  const [page, setPage] = React.useState<VmmNativeSessionListPage | null>(null)
  const [selectedIds, setSelectedIds] = React.useState<string[]>([])
  const [loading, setLoading] = React.useState(false)
  const [mutating, setMutating] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  // Refs prevent duplicate dispatch before React paints and ignore late results after leaving the view.
  // 引用防止 React 渲染前的重复派发，并忽略离开视图后的迟到结果。
  const pendingRef = React.useRef(false)
  const startedRef = React.useRef(false)
  const mountedRef = React.useRef(true)
  const occupied = busy || loading || mutating
  React.useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false } }, [])

  // Loads a new query, invalidating prior rows immediately; failures require an explicit fresh retry.
  // 加载新查询并立即失效旧行；失败后要求明确重新读取。
  const load = React.useCallback(async (nextQuery: VmmNativeSessionListQuery) => {
    if (pendingRef.current) return
    pendingRef.current = true
    setLoading(true)
    setQuery(nextQuery)
    setPage(null)
    setSelectedIds([])
    setError(null)
    try {
      const result = await onRunAction("native_session_list", { ...nextQuery })
      if (!mountedRef.current) return
      if (result?.data.kind === "native_session_list") setPage(result.data.value)
      else setError(t("memory.sessionList.failed"))
    } catch (failure) {
      if (mountedRef.current) setError(failure instanceof Error ? failure.message : t("memory.sessionList.failed"))
    } finally {
      pendingRef.current = false
      if (mountedRef.current) setLoading(false)
    }
  }, [onRunAction, t])

  // refreshedRevision advances after a successful detail mutation, preserving the visible query.
  // refreshedRevision 在详情修改成功后推进，并保留可见查询。
  const refreshedRevision = React.useRef(revision)
  React.useEffect(() => {
    if (revision === refreshedRevision.current || occupied || !query) return
    refreshedRevision.current = revision
    void load({ ...query, expectedStorageVersion: null })
  }, [revision, occupied, query, load])

  // Reads the catalog once before selecting the current binding; an explicit retry handles failure.
  // 选择当前绑定前读取一次目录；失败由显式重试处理。
  const initialize = React.useCallback(async () => {
    if (pendingRef.current) return
    pendingRef.current = true
    setLoading(true)
    setError(null)
    try {
      const result = await onRunAction("native_project_catalog")
      if (!mountedRef.current) return
      if (result?.data.kind !== "native_project_catalog") { setError(t("memory.sessionList.failed")); return }
      setCatalog(result.data.value)
      const ownerId = nativeProjectOwnerId(result.data.value, snapshot.selectedProjectId)
      pendingRef.current = false
      if (ownerId !== null) await load(nativeSessionInitialQuery(ownerId))
    } catch (failure) {
      if (mountedRef.current) setError(failure instanceof Error ? failure.message : t("memory.sessionList.failed"))
    } finally {
      pendingRef.current = false
      if (mountedRef.current) setLoading(false)
    }
  }, [load, onRunAction, snapshot.selectedProjectId, t])
  React.useEffect(() => {
    if (startedRef.current || busy) return
    startedRef.current = true
    void initialize()
  }, [busy, initialize])

  // Applies visible form fields as one fresh query; date upper bounds include the entire chosen day.
  // 将可见表单字段作为一次全新查询应用；日期上界包含所选整天。
  const applyFilters = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (occupied || !query) return
    const form = new FormData(event.currentTarget)
    try {
      const from = nativeSessionDateBound(String(form.get("from")), false)
      const before = nativeSessionDateBound(String(form.get("before")), true)
      if (from !== null && before !== null && from >= before) throw new Error(t("memory.sessionList.invalidDates"))
      void load({ ...query, search: String(form.get("search")), status: form.get("status") as VmmNativeSessionListQuery["status"], extractionStatus: form.get("extraction") as VmmNativeSessionListQuery["extractionStatus"], sort: form.get("sort") as VmmNativeSessionListQuery["sort"], lastActivityFromUnixMs: from, lastActivityBeforeUnixMs: before, offset: 0, expectedStorageVersion: null })
    } catch { setError(t("memory.sessionList.invalidDates")) }
  }

  // Selection is restricted to visible rows; mixed states are explained, never silently filtered.
  // 选择仅限可见行；混合状态明确说明，不静默过滤。
  const selectedSessions = page?.sessions.filter((session) => selectedIds.includes(session.sessionSourceId)) ?? []
  const archiveEligible = selectedSessions.length > 0 && (selectedSessions.every((session) => session.status === "active") || selectedSessions.every((session) => session.status === "archived"))
  // Recycling admits active and archived sources together, but never silently omits paused rows.
  // 回收允许混合活动和归档来源，但绝不静默忽略暂停记录。
  const removalEligible = selectedSessions.length > 0 && selectedSessions.every((session) => session.status === "active" || session.status === "archived")

  return <Card>
    <CardHeader><CardTitle>{t("memory.sessionList.title")}</CardTitle><CardDescription>{t("memory.sessionList.description")}</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      <div className="vmm-session-scope flex flex-wrap items-end gap-3">
        <label className="min-w-0 flex-1 space-y-1 text-sm">{t("memory.nativeRecords.project")}
          <Select disabled={occupied || !catalog?.projects.length} value={query ? String(query.vmmProjectId) : ""} onValueChange={(value) => { onScopeChange(); void load(nativeSessionInitialQuery(Number(value))) }}>
            <SelectTrigger aria-label={t("memory.nativeRecords.project")} className="w-full"><SelectValue placeholder={t("memory.nativeRecords.selectProject")} /></SelectTrigger>
            <SelectContent>{catalog?.projects.map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name}{project.name !== project.tag ? ` · ${project.tag}` : ""}</SelectItem>)}</SelectContent>
          </Select>
        </label>
        <Button variant="outline" disabled={occupied} onClick={() => { if (query) void load({ ...query, offset: 0, expectedStorageVersion: null }); else void initialize() }}>{t("memory.nativeRecords.refresh")}</Button>
        <Button variant="outline" disabled={occupied || !query} onClick={() => { if (query) onOpen({ projectId: query.vmmProjectId, sessionSourceId: null }) }}>{t("memory.sessionList.projectSources")}</Button>
      </div>
      {query ? <form key={query.vmmProjectId} className="vmm-session-filters" onSubmit={applyFilters} onChange={() => setSelectedIds([])}>
        <label className="space-y-1 text-sm">{t("memory.sessionList.search")}<Input name="search" maxLength={512} disabled={occupied} defaultValue="" /></label>
        <details className="vmm-session-filter-details"><summary>{t("memory.workspace.filters")}</summary><div className="vmm-filter-fields"><label className="space-y-1 text-sm">{t("memory.sessionList.status")}<Select name="status" disabled={occupied} defaultValue="all" onValueChange={() => setSelectedIds([])}><SelectTrigger aria-label={t("memory.sessionList.status")} className="w-full"><SelectValue /></SelectTrigger><SelectContent>{lifecycleOptions.map(([value, label]) => <SelectItem key={value} value={value}>{t(label)}</SelectItem>)}</SelectContent></Select></label>
        <label className="space-y-1 text-sm">{t("memory.sessionList.extraction")}<Select name="extraction" disabled={occupied} defaultValue="all" onValueChange={() => setSelectedIds([])}><SelectTrigger aria-label={t("memory.sessionList.extraction")} className="w-full"><SelectValue /></SelectTrigger><SelectContent>{extractionOptions.map(([value, label]) => <SelectItem key={value} value={value}>{t(label)}</SelectItem>)}</SelectContent></Select></label>
        <label className="space-y-1 text-sm">{t("memory.sessionList.from")}<Input name="from" type="date" disabled={occupied} /></label>
        <label className="space-y-1 text-sm">{t("memory.sessionList.before")}<Input name="before" type="date" disabled={occupied} /></label>
        <label className="space-y-1 text-sm">{t("memory.sessionList.sort")}<Select name="sort" disabled={occupied} defaultValue="last_activity_desc" onValueChange={() => setSelectedIds([])}>
          <SelectTrigger aria-label={t("memory.sessionList.sort")} className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent><SelectItem value="last_activity_desc">{t("memory.sessionList.newest")}</SelectItem><SelectItem value="last_activity_asc">{t("memory.sessionList.oldest")}</SelectItem><SelectItem value="title_asc">{t("memory.sessionList.titleSort")}</SelectItem></SelectContent>
        </Select></label>
        </div></details>
        <Button type="submit" variant="outline" disabled={occupied}>{t("memory.sessionList.apply")}</Button>
      </form> : null}
      {error ? <p role="alert" className="text-sm text-destructive">{error}</p> : null}
      {loading ? <p role="status">{t("memory.sessionList.loading")}</p> : null}
      {!loading && catalog?.projects.length === 0 ? <p>{t("memory.nativeRecords.noProjects")}</p> : null}
      {page && query ? <>
        <div className="flex flex-wrap items-center gap-3 text-sm">
          <label className="flex items-center gap-2"><input type="checkbox" disabled={occupied || page.sessions.length === 0} checked={page.sessions.length > 0 && selectedIds.length === page.sessions.length} onChange={(event) => setSelectedIds(event.target.checked ? page.sessions.map((session) => session.sessionSourceId) : [])} />{t("memory.sessionList.selectPage")}</label>
          <span>{t("memory.sessionList.selected").replace("{count}", String(selectedIds.length))}</span>
          {selectedSessions.length > 0 ? <VmmNativeSessionArchive key={JSON.stringify([page.projectId, selectedIds])} projectId={page.projectId} sessions={selectedSessions} busy={occupied} onBusyChange={setMutating} onRunAction={onRunAction} onRefresh={() => load({ ...query, offset: 0, expectedStorageVersion: null })} /> : null}
          {removalEligible ? <VmmNativeRemoval projectId={page.projectId} selection={{ kind: "sessions", ids: selectedIds }} sessionNames={new Map(selectedSessions.map((session) => [session.sessionSourceId, session.displayTitle ?? session.sessionSourceId]))} busy={occupied} onBusyChange={setMutating} onRunAction={onRunAction} onRefresh={() => load({ ...query, offset: 0, expectedStorageVersion: null })} /> : null}
          {selectedSessions.length > 0 && !archiveEligible ? <span>{t("memory.sessionList.mixed")}</span> : null}
        </div>
        <div className="divide-y rounded-lg border">
          {page.sessions.map((session) => <div key={session.sessionSourceId} className={`vmm-session-row flex items-start gap-3 p-3 ${selection?.sessionSourceId === session.sessionSourceId ? "selected" : ""}`}>
            <input className="mt-1" type="checkbox" aria-label={`${t("memory.sessionList.select")} ${session.displayTitle ?? session.sessionSourceId}`} disabled={occupied} checked={selectedIds.includes(session.sessionSourceId)} onChange={(event) => setSelectedIds((ids) => event.target.checked ? [...ids, session.sessionSourceId] : ids.filter((id) => id !== session.sessionSourceId))} />
            <div className="min-w-0 flex-1 space-y-2">
              <Button className="h-auto max-w-full whitespace-normal break-all p-0 text-left" variant="link" disabled={occupied} onClick={() => onOpen({ projectId: page.projectId, sessionSourceId: session.sessionSourceId })}>{session.displayTitle ?? session.sessionSourceId}</Button>
              <details className="text-xs text-muted-foreground"><summary>{t("memory.workspace.recordInfo")}</summary><p className="break-all font-mono">{session.sessionSourceId}</p></details>
              <NativeSessionStatus status={session.status} />
              <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                <span>{t("memory.nativeRecords.turns")} {session.turnCount}</span><span>{t("memory.nativeRecords.memories")} {session.memoryCount}</span><span>{t("memory.nativeRecords.profileNodes")} {session.profileNodeCount}</span>
                {extractionOptions.filter(([value]) => value !== "all").map(([value, label]) => {
                  // API counters use camelCase keys; the query enum uses snake_case by contract.
                  // 按契约，接口计数使用驼峰键，查询枚举使用下划线名称。
                  const counts = { pending: session.turnStatuses.pending, completed: session.turnStatuses.completed, processed_unknown: session.turnStatuses.processedUnknown, analysis_bypassed: session.turnStatuses.analysisBypassed, corrupted: session.turnStatuses.corrupted, all: 0 }
                  return counts[value] > 0 ? <span key={value}>{t(label)} {counts[value]}</span> : null
                })}
              </div>
              {session.lastActivityAtUnixMs != null ? <p className="text-xs text-muted-foreground">{t("memory.nativeRecords.lastActivity")} {new Date(session.lastActivityAtUnixMs).toLocaleString()}</p> : null}
            </div>
          </div>)}
          {page.sessions.length === 0 ? <p className="p-4 text-sm text-muted-foreground">{t("memory.sessionList.empty")}</p> : null}
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
          <span>{t("memory.sessionList.total").replace("{total}", String(page.total)).replace("{page}", String(Math.floor(page.offset / page.limit) + 1))}</span>
          <div className="flex gap-2">
            <Button variant="outline" disabled={occupied || page.offset === 0} onClick={() => void load({ ...query, offset: Math.max(0, page.offset - page.limit), expectedStorageVersion: page.storageVersion })}>{t("memory.sessionList.previous")}</Button>
            <Button variant="outline" disabled={occupied || page.offset + page.sessions.length >= page.total} onClick={() => void load({ ...query, offset: page.offset + page.limit, expectedStorageVersion: page.storageVersion })}>{t("memory.sessionList.next")}</Button>
          </div>
        </div>
      </> : null}
    </CardContent>
  </Card>
}
