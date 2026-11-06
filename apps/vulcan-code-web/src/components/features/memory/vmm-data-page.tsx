import * as React from "react"
import {
  Archive,
  ChevronLeft,
  ChevronRight,
  Database,
  RefreshCw,
  RotateCcw,
  Search,
  Trash2,
  UserRound,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/ui-kit/primitives/dialog"
import { PageEmptyState } from "@/components/page"
import { Input } from "@/ui-kit/primitives/input"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/ui-kit/primitives/input-group"
import { Label } from "@/ui-kit/primitives/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/ui-kit/primitives/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/ui-kit/primitives/tabs"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { formatMessage, useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmMemorySnapshot } from "@/types"

import { VmmContentPage } from "./vmm-content-page"
import { formatBytes, formatDateTime } from "@/lib/display-format"
import "@/ui-kit/styles/features/settings-forms.css"
import "@/ui-kit/styles/features/memory.css"

// ManagementPage is one cursor-based VMM management response.
// ManagementPage 是一页基于游标的 VMM 管理响应。
interface ManagementPage<T> {
  items: T[]
  next_cursor?: string
  has_more: boolean
}

// ManagementCollection is one non-paginated management list envelope.
// ManagementCollection 是一个不分页的管理列表信封。
interface ManagementCollection<T> {
  items: T[]
}

// ManagementCapabilities describes the independently authenticated management plane.
// ManagementCapabilities 描述独立鉴权的管理面能力。
interface ManagementCapabilities {
  protocol_version: number
  service_version: string
  storage_mode: string
  max_page_size: number
  content_chunk_bytes: number
  restore_supported: boolean
  full_text_index_status: string
}

// ManagementFilterSnapshot contains one exact user-visible filter state.
// ManagementFilterSnapshot 包含一组准确的用户可见筛选状态。
interface ManagementFilterSnapshot {
  query: string
  status: string
  turnStatus: string
  userId: string
  projectId: string
  createdFrom: string
  createdTo: string
}

// ManagementIdentity is one human-readable VMM identity.
// ManagementIdentity 是一个人类可读的 VMM 身份。
interface ManagementIdentity {
  id: string
  name: string
}

// ManagementProject is one human-readable VMM project.
// ManagementProject 是一个人类可读的 VMM 项目。
interface ManagementProject extends ManagementIdentity {
  path: string
}

// ManagementSession is one bounded session row returned by the management API.
// ManagementSession 是管理 API 返回的一条有界会话记录。
interface ManagementSession {
  session_id: string
  display_title: string
  user: ManagementIdentity
  project: ManagementProject
  status: string
  created_at: string
  updated_at: string
  historical_turn_count: number
  visible_turn_count: number
  pending_turn_count: number
  active_memory_count: number
  profile_reference_count: number
  local_session_id?: string
  active_runtime?: boolean
}

// ManagementTurn is one bounded turn row returned by the management API.
// ManagementTurn 是管理 API 返回的一条有界回合记录。
interface ManagementTurn {
  turn_id: string
  session_id: string
  display_excerpt: string
  extraction_status: string
  created_at: string
  content_bytes: number
  derived_memories: number
  profile_references: number
  user?: ManagementIdentity
  project?: ManagementProject
}

// ManagementTurnContent is one UTF-8-safe content fragment.
// ManagementTurnContent 是一个 UTF-8 安全的内容片段。
interface ManagementTurnContent {
  turn_id: string
  section: "user" | "assistant" | "timeline"
  offset: number
  next_offset: number
  has_more: boolean
  chunk: string
}

// ManagementImpact is the server-authoritative destructive-action preview.
// ManagementImpact 是服务端权威的破坏性操作影响预览。
interface ManagementImpact {
  preview_token: string
  action: string
  session_count: number
  turn_count: number
  memory_count: number
  vector_count: number
  profile_node_count: number
  pending_turn_count: number
  hot_window_turn_count: number
  protected_count: number
  required_confirmation?: string
}

// ManagementRecycleBatch is one human or system recycle-bin batch.
// ManagementRecycleBatch 是一条人工或系统回收站批次。
interface ManagementRecycleBatch {
  batch_id: string
  source: string
  target_type?: string
  restorable: boolean
  state: string
  reason: string
  session_count: number
  turn_count: number
  memory_count: number
  profile_count: number
  recycled_at: string
  expires_at?: string
}

// VmmDataPageProps defines the memory-data workspace inputs.
// VmmDataPageProps 定义记忆数据工作区输入。
interface VmmDataPageProps {
  busy: boolean
  // notice preserves controller feedback for the nested direct-memory editor.
  // notice 为内嵌直接记忆编辑器保留控制器反馈。
  notice?: string
  snapshot: VmmMemorySnapshot
  result: VmmMemoryActionResult | null
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// VmmDataPage renders session, turn, recycle-bin, and direct-memory management in one workspace.
// VmmDataPage 在同一工作区渲染会话、回合、回收站和直接记忆管理。
export function VmmDataPage(props: VmmDataPageProps) {
  const { t, locale } = useI18n()
  const [sessions, setSessions] = React.useState<ManagementSession[]>([])
  const [sessionCursor, setSessionCursor] = React.useState("")
  const [sessionHasMore, setSessionHasMore] = React.useState(false)
  const [query, setQuery] = React.useState("")
  const [status, setStatus] = React.useState("all")
  const [turnStatus, setTurnStatus] = React.useState("all")
  const [userId, setUserId] = React.useState("all")
  const [projectId, setProjectId] = React.useState("all")
  const [createdFrom, setCreatedFrom] = React.useState("")
  const [createdTo, setCreatedTo] = React.useState("")
  const [users, setUsers] = React.useState<ManagementIdentity[]>([])
  const [projects, setProjects] = React.useState<ManagementProject[]>([])
  const [selectedSessionIds, setSelectedSessionIds] = React.useState<Set<string>>(() => new Set())
  const [openSession, setOpenSession] = React.useState<ManagementSession | null>(null)
  const [turns, setTurns] = React.useState<ManagementTurn[]>([])
  const [turnCursor, setTurnCursor] = React.useState("")
  const [turnHasMore, setTurnHasMore] = React.useState(false)
  const [globalTurns, setGlobalTurns] = React.useState<ManagementTurn[]>([])
  const [globalTurnCursor, setGlobalTurnCursor] = React.useState("")
  const [globalTurnHasMore, setGlobalTurnHasMore] = React.useState(false)
  const [openTurn, setOpenTurn] = React.useState<ManagementTurn | null>(null)
  const [turnContent, setTurnContent] = React.useState<Record<"user" | "assistant", ManagementTurnContent | null>>({ user: null, assistant: null })
  const [recycleBatches, setRecycleBatches] = React.useState<ManagementRecycleBatch[]>([])
  const [impact, setImpact] = React.useState<ManagementImpact | null>(null)
  const [purgePhrase, setPurgePhrase] = React.useState("")
  const [error, setError] = React.useState("")
  const [capabilities, setCapabilities] = React.useState<ManagementCapabilities | null>(null)
  const [updatedAt, setUpdatedAt] = React.useState("")
  const initialLoadIssued = React.useRef(false)

  // loadSessions refreshes or appends one bounded session page.
  // loadSessions 刷新或追加一页有界会话数据。
  const loadSessions = React.useCallback(async (append: boolean, override?: ManagementFilterSnapshot) => {
    const effective = override ?? { query, status, turnStatus, userId, projectId, createdFrom, createdTo }
    setError("")
    try {
      const result = await props.onRunAction("management_list_sessions", {
        filters: {
          query: effective.query.trim() || undefined,
          status: effective.status === "all" ? "all" : effective.status,
          userId: effective.userId === "all" ? undefined : effective.userId,
          projectId: effective.projectId === "all" ? undefined : effective.projectId,
          createdFrom: managementDateBoundary(effective.createdFrom, "start"),
          createdTo: managementDateBoundary(effective.createdTo, "end"),
          cursor: append ? sessionCursor : undefined,
          limit: 30,
          sort: "updated_desc",
        },
      })
      const page = managementPage<ManagementSession>(result, t("memory.data.invalidResponse"))
      if (!page) return
      setSessions((current) => append ? [...current, ...page.items] : page.items)
      setSessionCursor(page.next_cursor ?? "")
      setSessionHasMore(page.has_more)
      if (!append) setSelectedSessionIds(new Set())
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [createdFrom, createdTo, projectId, props.onRunAction, query, sessionCursor, status, t, turnStatus, userId])

  // loadScopes loads human-readable users and projects for shared filters.
  // loadScopes 为公共筛选器加载人类可读用户与项目。
  const loadScopes = React.useCallback(async () => {
    try {
      const userResult = await props.onRunAction("management_list_users")
      const projectResult = await props.onRunAction("management_list_projects")
      setUsers(managementItems<ManagementIdentity>(userResult, t("memory.data.invalidResponse")))
      setProjects(managementItems<ManagementProject>(projectResult, t("memory.data.invalidResponse")))
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t])

  // loadCapabilities verifies the independent management plane and retains its bounded contract summary.
  // loadCapabilities 验证独立管理面并保留其有界契约摘要。
  const loadCapabilities = React.useCallback(async () => {
    try {
      const result = await props.onRunAction("management_capabilities")
      const value = managementValue<ManagementCapabilities>(result, t("memory.data.invalidResponse"))
      if (value) setCapabilities(value)
    } catch (requestError) {
      setCapabilities(null)
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t])

  // loadGlobalTurns refreshes or appends the global bounded turn workspace.
  // loadGlobalTurns 刷新或追加全局有界回合工作区。
  const loadGlobalTurns = React.useCallback(async (append: boolean, override?: ManagementFilterSnapshot) => {
    const effective = override ?? { query, status, turnStatus, userId, projectId, createdFrom, createdTo }
    setError("")
    try {
      const result = await props.onRunAction("management_list_turns", {
        filters: {
          query: effective.query.trim() || undefined,
          status: effective.turnStatus === "all" ? undefined : effective.turnStatus,
          userId: effective.userId === "all" ? undefined : effective.userId,
          projectId: effective.projectId === "all" ? undefined : effective.projectId,
          createdFrom: managementDateBoundary(effective.createdFrom, "start"),
          createdTo: managementDateBoundary(effective.createdTo, "end"),
          cursor: append ? globalTurnCursor : undefined,
          limit: 30,
          sort: "id_desc",
        },
      })
      const page = managementPage<ManagementTurn>(result, t("memory.data.invalidResponse"))
      if (!page) return
      setGlobalTurns((current) => append ? [...current, ...page.items] : page.items)
      setGlobalTurnCursor(page.next_cursor ?? "")
      setGlobalTurnHasMore(page.has_more)
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [createdFrom, createdTo, globalTurnCursor, projectId, props.onRunAction, query, status, t, turnStatus, userId])

  // openTurnDetail reads bounded user and assistant sections on demand.
  // openTurnDetail 按需读取有界的用户与助手内容区段。
  // Content reads use the chunk size the management service declares, never a client-side copy.
  // 内容读取使用管理服务声明的分片大小，绝不使用客户端副本。
  const openTurnDetail = React.useCallback(async (turn: ManagementTurn) => {
    setError("")
    setOpenTurn(turn)
    setTurnContent({ user: null, assistant: null })
    try {
      const userResult = await props.onRunAction("management_get_turn_content", { turnId: turn.turn_id, filters: { section: "user", offset: 0, limitBytes: capabilities?.content_chunk_bytes } })
      const assistantResult = await props.onRunAction("management_get_turn_content", { turnId: turn.turn_id, filters: { section: "assistant", offset: 0, limitBytes: capabilities?.content_chunk_bytes } })
      setTurnContent({
        user: managementValue<ManagementTurnContent>(userResult, t("memory.data.invalidResponse")),
        assistant: managementValue<ManagementTurnContent>(assistantResult, t("memory.data.invalidResponse")),
      })
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [capabilities?.content_chunk_bytes, props.onRunAction, t])

  // loadTurns refreshes or appends turns for one exact selected session.
  // loadTurns 刷新或追加一个准确选中会话的回合。
  const loadTurns = React.useCallback(async (session: ManagementSession, append: boolean) => {
    setError("")
    try {
      const result = await props.onRunAction("management_list_session_turns", {
        sessionId: session.session_id,
        filters: {
          cursor: append ? turnCursor : undefined,
          limit: 30,
          sort: "id_desc",
        },
      })
      const page = managementPage<ManagementTurn>(result, t("memory.data.invalidResponse"))
      if (!page) return
      setOpenSession(session)
      setTurns((current) => append ? [...current, ...page.items] : page.items)
      setTurnCursor(page.next_cursor ?? "")
      setTurnHasMore(page.has_more)
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t, turnCursor])

  // loadRecycleBatches refreshes the bounded recycle-bin page.
  // loadRecycleBatches 刷新有界回收站页面。
  const loadRecycleBatches = React.useCallback(async () => {
    setError("")
    try {
      const result = await props.onRunAction("management_list_recycle_batches", {
        filters: { limit: 100 },
      })
      const page = managementPage<ManagementRecycleBatch>(result, t("memory.data.invalidResponse"))
      if (page) setRecycleBatches(page.items)
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t])

  // refreshWorkspace refreshes the management contract and the visible scope/session caches together.
  // refreshWorkspace 会一并刷新管理契约以及可见的范围和会话缓存。
  const refreshWorkspace = React.useCallback(async () => {
    await loadCapabilities()
    await loadScopes()
    await loadSessions(false)
    setUpdatedAt(new Date().toISOString())
  }, [loadCapabilities, loadScopes, loadSessions])

  // resetFilters clears every shared filter and reloads both session and turn workspaces with that exact empty state.
  // resetFilters 清空全部共享筛选器，并使用这组准确空状态重新加载会话与回合工作区。
  const resetFilters = React.useCallback(async () => {
    const empty: ManagementFilterSnapshot = {
      query: "",
      status: "all",
      turnStatus: "all",
      userId: "all",
      projectId: "all",
      createdFrom: "",
      createdTo: "",
    }
    setQuery(empty.query)
    setStatus(empty.status)
    setTurnStatus(empty.turnStatus)
    setUserId(empty.userId)
    setProjectId(empty.projectId)
    setCreatedFrom(empty.createdFrom)
    setCreatedTo(empty.createdTo)
    await loadSessions(false, empty)
    await loadGlobalTurns(false, empty)
  }, [loadGlobalTurns, loadSessions])

  React.useEffect(() => {
    if (!props.snapshot.runtime.processReady) {
      initialLoadIssued.current = false
      setCapabilities(null)
      return
    }
    if (initialLoadIssued.current) return
    initialLoadIssued.current = true
    void refreshWorkspace()
  }, [props.snapshot.runtime.processReady, refreshWorkspace])

  // previewSelection asks VMM to calculate live impact before a mutation dialog opens.
  // previewSelection 在变更对话框打开前要求 VMM 计算实时影响。
  const previewSelection = React.useCallback(async (
    targetType: "session" | "turn",
    action: "archive" | "unarchive" | "recycle" | "restart",
    ids: string[],
  ) => {
    if (ids.length === 0) return
    setError("")
    try {
      const result = await props.onRunAction("management_create_removal_preview", {
          targetType,
        targetIds: ids,
        action,
        source: "vulcan-code",
      })
      const value = managementValue<ManagementImpact>(result, t("memory.data.invalidResponse"))
      if (value) setImpact(value)
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t])

  // executeImpact performs the exact preview-bound action and refreshes affected lists.
  // executeImpact 执行准确的预览约束动作并刷新受影响列表。
  const executeImpact = React.useCallback(async () => {
    if (!impact) return
    setError("")
    try {
      // completed is the actual mutation acknowledgement; a rejected concurrent call returns null.
      // completed 是实际变更确认；被拒绝的并发调用返回 null。
      const completed = await props.onRunAction(impact.action === "purge" ? "management_purge" : "management_execute_removal", {
        previewToken: impact.preview_token,
        // The typed phrase is a localized gesture; the service validates only its own required token.
        // 输入短语是本地化确认手势；服务只校验它自己签发的确认令牌。
        confirmText: impact.action === "purge" ? impact.required_confirmation : undefined,
      })
      if (!completed) return
      setImpact(null)
      setPurgePhrase("")
      await loadSessions(false)
      await loadGlobalTurns(false)
      if (openSession) await loadTurns(openSession, false)
      await loadRecycleBatches()
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [impact, loadGlobalTurns, loadRecycleBatches, loadSessions, loadTurns, openSession, props.onRunAction])

  // restoreBatch restores one explicitly restorable batch.
  // restoreBatch 恢复一个明确可恢复的批次。
  const restoreBatch = React.useCallback(async (batchId: string) => {
    setError("")
    try {
      // completed prevents an unexecuted restore from being rendered as a successful refresh.
      // completed 防止未执行的恢复被渲染为成功刷新。
      const completed = await props.onRunAction("management_restore_batch", { batchId })
      if (!completed) return
      await loadSessions(false)
      await loadRecycleBatches()
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [loadRecycleBatches, loadSessions, props.onRunAction])

  // previewPurge creates the mandatory permanent-delete preview for one batch.
  // previewPurge 为一个批次创建强制永久删除预览。
  const previewPurge = React.useCallback(async (batchId: string) => {
    setError("")
    try {
      const result = await props.onRunAction("management_create_removal_preview", {
        targetType: "recycle_batch",
        targetIds: [batchId],
        action: "purge",
        source: "vulcan-code",
      })
      const value = managementValue<ManagementImpact>(result, t("memory.data.invalidResponse"))
      if (value) setImpact(value)
    } catch (requestError) {
      setError(managementErrorMessage(requestError))
    }
  }, [props.onRunAction, t])

  const selectedSessions = sessions.filter((session) => selectedSessionIds.has(session.session_id))
  const allSelectedActive = selectedSessions.length > 0 && selectedSessions.every((session) => session.status === "active")
  const allSelectedArchived = selectedSessions.length > 0 && selectedSessions.every((session) => session.status === "archived")
  const allSelectedForgotten = selectedSessions.length > 0 && selectedSessions.every((session) => session.status === "forgotten")
  const selectionAllowsRecycle = selectedSessions.length > 0 && selectedSessions.every((session) => session.status === "active" || session.status === "archived")
  const selectionHasMixedLifecycle = selectedSessions.length > 0
    && !allSelectedActive
    && !allSelectedArchived
    && !allSelectedForgotten

  const postActionUnavailable = props.snapshot.runtime.processReady
    && !props.snapshot.runtime.postactionReady
  const postActionMessage = props.snapshot.runtime.postactionError?.trim()
    || t("memory.data.extractionRuntimeWaiting")

  return (
    <div className="vk-form-loose">
      <div className="vmm-page-toolbar">
        <div className="vk-form-inline">
          <Badge variant={capabilities ? "default" : "destructive"}>{capabilities ? t("memory.data.managementReady") : t("memory.data.managementUnavailable")}</Badge>
          {capabilities ? <Badge variant="outline">{capabilities.protocol_version} · {capabilities.storage_mode}</Badge> : null}
          {capabilities?.full_text_index_status ? <Badge variant="outline">{t("memory.data.fullTextIndex")}: {capabilities.full_text_index_status}</Badge> : null}
          {updatedAt ? <span className="vk-form-note">{t("memory.data.updatedAt").replace("{time}", formatManagementTime(updatedAt, locale))}</span> : null}
          <Button aria-label={t("memory.data.refresh")} disabled={props.busy} onClick={() => void refreshWorkspace()} size="icon" variant="outline"><RefreshCw className="vk-icon" /></Button>
        </div>
      </div>
      {error ? (
        <Alert variant="destructive">
          <AlertTitle>{t("conversation.metadata.error")}</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}
      {postActionUnavailable ? (
        <Alert variant={props.snapshot.runtime.postactionError ? "destructive" : "default"}>
          <AlertTitle>{t("memory.data.extractionRuntimeTitle")}</AlertTitle>
          <AlertDescription>{postActionMessage}</AlertDescription>
        </Alert>
      ) : null}
      <Tabs defaultValue="sessions" onValueChange={(value) => {
        if (value === "recycle") void loadRecycleBatches()
        if (value === "turns") void loadGlobalTurns(false)
      }}>
        <div className="vk-mem-tabstrip">
          <TabsList>
            <TabsTrigger value="sessions" disabled={props.busy}>{t("memory.data.sessions")}</TabsTrigger>
            <TabsTrigger value="turns" disabled={props.busy}>{t("memory.data.turns")}</TabsTrigger>
            <TabsTrigger value="recycle" disabled={props.busy}>{t("memory.data.recycleBin")}</TabsTrigger>
            <TabsTrigger value="memories" disabled={props.busy}>{t("memory.data.memoryEntries")}</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value="sessions" className="vk-form-stack">
          {openSession ? (
            <TurnWorkspace
              busy={props.busy}
              hasMore={turnHasMore}
              onBack={() => {
                setOpenSession(null)
                setTurns([])
              }}
              onLoadMore={() => loadTurns(openSession, true)}
              onOpenTurn={(turn) => void openTurnDetail(turn)}
              onRecycleTurn={(turn) => void previewSelection("turn", "recycle", [turn.turn_id])}
              session={openSession}
              turns={turns}
            />
          ) : (
            <>
              <div className="vk-form-inline">
                <InputGroup className="vk-mem-search">
                  <InputGroupAddon align="inline-start"><Search aria-hidden="true" /></InputGroupAddon>
                  <InputGroupInput
                    aria-label={t("memory.data.searchPlaceholder")}
                    onChange={(event) => setQuery(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void loadSessions(false)
                    }}
                    placeholder={t("memory.data.searchPlaceholder")}
                    value={query}
                  />
                </InputGroup>
                <Select value={status} onValueChange={setStatus}>
                  <SelectTrigger className="vk-form-w-sm"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t("memory.data.statusAll")}</SelectItem>
                    <SelectItem value="active">{t("memory.data.statusActive")}</SelectItem>
                    <SelectItem value="archived">{t("memory.data.statusArchived")}</SelectItem>
                    <SelectItem value="recycled">{t("memory.data.statusRecycled")}</SelectItem>
                    <SelectItem value="forgotten">{t("memory.data.statusForgotten")}</SelectItem>
                  </SelectContent>
                </Select>
                <Select value={userId} onValueChange={setUserId}>
                  <SelectTrigger className="vk-form-w-md"><UserRound className="vk-icon" /><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t("memory.data.allUsers")}</SelectItem>
                    {users.map((user) => <SelectItem key={user.id} value={user.id}>{user.name}</SelectItem>)}
                  </SelectContent>
                </Select>
                <Select value={projectId} onValueChange={setProjectId}>
                  <SelectTrigger className="vk-form-w-lg"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t("memory.data.allProjects")}</SelectItem>
                    {projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.path}</SelectItem>)}
                  </SelectContent>
                </Select>
                <Input aria-label={t("memory.data.createdFrom")} className="vk-form-w-sm" max={createdTo || undefined} onChange={(event) => setCreatedFrom(event.target.value)} type="date" value={createdFrom} />
                <Input aria-label={t("memory.data.createdTo")} className="vk-form-w-sm" min={createdFrom || undefined} onChange={(event) => setCreatedTo(event.target.value)} type="date" value={createdTo} />
                <Button disabled={props.busy} onClick={() => void resetFilters()} variant="ghost">{t("memory.data.resetFilters")}</Button>
                <Button disabled={props.busy} onClick={() => void loadSessions(false)} variant="outline">
                  <RefreshCw className="vk-icon" />{t("memory.data.refresh")}
                </Button>
              </div>
              {selectedSessionIds.size > 0 ? (
                <Card>
                  <CardContent className="vk-mem-selection">
                    <span className="vk-form-strong">{t("memory.data.selected").replace("{count}", String(selectedSessionIds.size))}</span>
                    {selectionHasMixedLifecycle ? <span className="vk-form-note">{t("memory.data.mixedLifecycle")}</span> : null}
                    {allSelectedForgotten ? (
                        <Button disabled={props.busy} onClick={() => void previewSelection("session", "restart", [...selectedSessionIds])} variant="outline">
                        <RotateCcw className="vk-icon" />{t("memory.data.restartMemory")}
                      </Button>
                    ) : (
                      <>
                        <Button disabled={props.busy || (!allSelectedActive && !allSelectedArchived)} onClick={() => void previewSelection("session", allSelectedArchived ? "unarchive" : "archive", [...selectedSessionIds])} variant="outline">
                          <Archive className="vk-icon" />{allSelectedArchived ? t("memory.data.unarchive") : t("memory.data.archive")}
                        </Button>
                        <Button disabled={props.busy || !selectionAllowsRecycle} onClick={() => void previewSelection("session", "recycle", [...selectedSessionIds])} variant="destructive">
                          <Trash2 className="vk-icon" />{t("memory.data.recycle")}
                        </Button>
                      </>
                    )}
                  </CardContent>
                </Card>
              ) : null}
              <SessionTable
                busy={props.busy}
                onOpen={(session) => void loadTurns(session, false)}
                onSelectionChange={setSelectedSessionIds}
                selected={selectedSessionIds}
                sessions={sessions}
              />
              {sessionHasMore ? (
                <Button disabled={props.busy} onClick={() => void loadSessions(true)} variant="outline">
                  <ChevronRight className="vk-icon" />{t("memory.data.loadMore")}
                </Button>
              ) : null}
            </>
          )}
        </TabsContent>
        <TabsContent value="turns" className="vk-form-stack">
          <div className="vk-form-inline">
            <InputGroup className="vk-mem-search">
              <InputGroupAddon align="inline-start"><Search aria-hidden="true" /></InputGroupAddon>
              <InputGroupInput aria-label={t("memory.data.searchTurnPlaceholder")} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void loadGlobalTurns(false) }} placeholder={t("memory.data.searchTurnPlaceholder")} value={query} />
            </InputGroup>
            <Select value={userId} onValueChange={setUserId}>
              <SelectTrigger className="vk-form-w-md"><UserRound className="vk-icon" /><SelectValue /></SelectTrigger>
              <SelectContent><SelectItem value="all">{t("memory.data.allUsers")}</SelectItem>{users.map((user) => <SelectItem key={user.id} value={user.id}>{user.name}</SelectItem>)}</SelectContent>
            </Select>
            <Select value={projectId} onValueChange={setProjectId}>
              <SelectTrigger className="vk-form-w-lg"><SelectValue /></SelectTrigger>
              <SelectContent><SelectItem value="all">{t("memory.data.allProjects")}</SelectItem>{projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.path}</SelectItem>)}</SelectContent>
            </Select>
            <Select value={turnStatus} onValueChange={setTurnStatus}>
              <SelectTrigger className="vk-form-w-sm"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("memory.data.allStatuses")}</SelectItem>
                <SelectItem value="pending">{t("memory.data.extractionPending")}</SelectItem>
                <SelectItem value="extracted">{t("memory.data.extractionCompleted")}</SelectItem>
              </SelectContent>
            </Select>
            <Input aria-label={t("memory.data.createdFrom")} className="vk-form-w-sm" max={createdTo || undefined} onChange={(event) => setCreatedFrom(event.target.value)} type="date" value={createdFrom} />
            <Input aria-label={t("memory.data.createdTo")} className="vk-form-w-sm" min={createdFrom || undefined} onChange={(event) => setCreatedTo(event.target.value)} type="date" value={createdTo} />
            <Button disabled={props.busy} onClick={() => void resetFilters()} variant="ghost">{t("memory.data.resetFilters")}</Button>
            <Button disabled={props.busy} onClick={() => void loadGlobalTurns(false)} variant="outline"><RefreshCw className="vk-icon" />{t("memory.data.refresh")}</Button>
          </div>
          <TurnList
            busy={props.busy}
            onOpen={(turn) => void openTurnDetail(turn)}
            onRecycle={(turn) => void previewSelection("turn", "recycle", [turn.turn_id])}
            turns={globalTurns}
          />
          {globalTurnHasMore ? <Button disabled={props.busy} onClick={() => void loadGlobalTurns(true)} variant="outline">{t("memory.data.loadMore")}</Button> : null}
        </TabsContent>
        <TabsContent value="recycle">
          <RecycleBin
            batches={recycleBatches}
            busy={props.busy}
            onPurge={previewPurge}
            onRestore={restoreBatch}
          />
        </TabsContent>
        <TabsContent value="memories">
          <VmmContentPage busy={props.busy} notice={props.notice} onRunAction={props.onRunAction} result={props.result} snapshot={props.snapshot} />
        </TabsContent>
      </Tabs>
      <ImpactDialog
        busy={props.busy}
        impact={impact}
        onClose={() => {
          setImpact(null)
          setPurgePhrase("")
        }}
        onConfirm={() => void executeImpact()}
        onPurgePhraseChange={setPurgePhrase}
        purgePhrase={purgePhrase}
      />
      <TurnDetailDialog busy={props.busy} content={turnContent} onClose={() => setOpenTurn(null)} onLoadMore={async (section) => {
        const current = turnContent[section]
        if (!openTurn || !current?.has_more) return
        setError("")
        try {
          const result = await props.onRunAction("management_get_turn_content", { turnId: openTurn.turn_id, filters: { section, offset: current.next_offset, limitBytes: capabilities?.content_chunk_bytes } })
          const next = managementValue<ManagementTurnContent>(result, t("memory.data.invalidResponse"))
          if (next) setTurnContent((value) => ({ ...value, [section]: { ...next, chunk: `${current.chunk}${next.chunk}` } }))
        } catch (requestError) {
          setError(managementErrorMessage(requestError))
        }
      }} openTurn={openTurn} />
    </div>
  )
}

// SessionTable renders human-readable session rows with batch selection.
// SessionTable 渲染可批量选择的人类可读会话记录。
function SessionTable(props: {
  busy: boolean
  sessions: ManagementSession[]
  selected: Set<string>
  onSelectionChange: (next: Set<string>) => void
  onOpen: (session: ManagementSession) => void
}) {
  const { t, locale } = useI18n()
  if (props.sessions.length === 0) {
    return <DataEmpty description={t("memory.data.noSessions")} />
  }
  return (
    <>
      <div className="vk-form-stack vk-mem-narrow-only">
        {props.sessions.map((session) => (
          <Card key={session.session_id}>
            <CardContent className="vk-form-stack">
              <div className="vk-mem-check-row">
                <Checkbox
                  aria-label={session.display_title}
                  checked={props.selected.has(session.session_id)}
                  disabled={props.busy || session.active_runtime}
                  onCheckedChange={(checked) => {
                    const next = new Set(props.selected)
                    if (checked) next.add(session.session_id)
                    else next.delete(session.session_id)
                    props.onSelectionChange(next)
                  }}
                />
                <div className="vk-form-fill">
                  <div className="vk-mem-clamp2 vk-form-em">{session.display_title}</div>
                  <div className="vk-form-meta">
                    <span>{session.user.name}</span>
                    <span>·</span>
                    <span>{formatManagementTime(session.updated_at, locale)}</span>
                    <Badge variant="outline">{managementSessionStatusLabel(t, session.status)}</Badge>
                    {session.active_runtime ? <Badge variant="destructive">{t("memory.data.activeRuntime")}</Badge> : null}
                  </div>
                </div>
              </div>
              <div className="vk-form-note vk-truncate">{session.project.path}</div>
              <div className="vk-form-meta">
                <span>{t("memory.data.turnCount")}: {session.visible_turn_count}</span>
                <span>{t("memory.data.memoryCount")}: {session.active_memory_count}</span>
                <Button className="vk-mem-push" disabled={props.busy} onClick={() => props.onOpen(session)} size="sm" variant="outline">
                  {t("memory.data.viewTurns")}<ChevronRight className="vk-icon" />
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
      <Card className="vk-mem-wide-only">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead className="vk-mem-col-check" />
            <TableHead>{t("memory.data.sessions")}</TableHead>
            <TableHead>{t("memory.identity.projects")}</TableHead>
            <TableHead>{t("memory.data.turnCount")}</TableHead>
            <TableHead>{t("memory.data.memoryCount")}</TableHead>
            <TableHead className="vk-mem-col-action" />
          </TableRow>
        </TableHeader>
        <TableBody>
          {props.sessions.map((session) => (
            <TableRow key={session.session_id}>
              <TableCell>
                <Checkbox
                  aria-label={session.display_title}
                  checked={props.selected.has(session.session_id)}
                  disabled={props.busy || session.active_runtime}
                  onCheckedChange={(checked) => {
                    const next = new Set(props.selected)
                    if (checked) next.add(session.session_id)
                    else next.delete(session.session_id)
                    props.onSelectionChange(next)
                  }}
                />
              </TableCell>
              <TableCell className="vk-mem-col-title">
                <div className="vk-form-name">{session.display_title}</div>
                <div className="vk-form-meta">
                  <span>{session.user.name}</span>
                  <span>·</span>
                  <span>{formatManagementTime(session.updated_at, locale)}</span>
                  <Badge variant="outline">{managementSessionStatusLabel(t, session.status)}</Badge>
                  {session.active_runtime ? <Badge variant="destructive">{t("memory.data.activeRuntime")}</Badge> : null}
                </div>
              </TableCell>
              <TableCell className="vk-mem-col-path" title={session.project.path}>{session.project.path}</TableCell>
              <TableCell className="vk-num">{session.visible_turn_count}</TableCell>
              <TableCell className="vk-num">{session.active_memory_count}</TableCell>
              <TableCell>
                <Button disabled={props.busy} onClick={() => props.onOpen(session)} size="sm" variant="ghost">
                  {t("memory.data.viewTurns")}<ChevronRight className="vk-icon" />
                </Button>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      </Card>
    </>
  )
}

// TurnWorkspace renders one session's bounded turn history.
// TurnWorkspace 渲染一个会话的有界回合历史。
function TurnWorkspace(props: {
  busy: boolean
  session: ManagementSession
  turns: ManagementTurn[]
  hasMore: boolean
  onBack: () => void
  onLoadMore: () => void
  onOpenTurn: (turn: ManagementTurn) => void
  onRecycleTurn: (turn: ManagementTurn) => void
}) {
  const { t, locale } = useI18n()
  return (
    <div className="vk-form-stack">
      <div className="vk-form-inline">
        <Button disabled={props.busy} onClick={props.onBack} size="sm" variant="ghost"><ChevronLeft className="vk-icon" />{t("memory.data.backToSessions")}</Button>
        <div className="vk-form-cell">
          <h3 className="vk-form-heading vk-truncate">{props.session.display_title}</h3>
          <p className="vk-form-note vk-truncate">{props.session.project.path}</p>
        </div>
      </div>
      {props.turns.length === 0 ? <DataEmpty description={t("memory.data.noTurns")} /> : (
        <div className="vk-form-stack">
          {props.turns.map((turn) => (
            <Card key={turn.turn_id} variant="interactive">
              <CardHeader>
                <div className="vk-form-row">
                  <CardTitle className="vk-mem-clamp2 vk-form-sm">{turn.display_excerpt}</CardTitle>
                  <Badge variant="outline">{managementExtractionStatusLabel(t, turn.extraction_status)}</Badge>
                </div>
              </CardHeader>
              <CardContent className="vk-form-meta">
                <span>{formatManagementTime(turn.created_at, locale)}</span>
                <span>{formatBytes(turn.content_bytes)}</span>
                <span>{t("memory.data.memoryCount")}: {turn.derived_memories}</span>
                <span>{t("memory.data.profileCount")}: {turn.profile_references}</span>
                <div className="vk-form-inline vk-mem-push">
                  <Button disabled={props.busy} onClick={() => props.onOpenTurn(turn)} size="sm" variant="outline">{t("memory.data.details")}</Button>
                  <Button disabled={props.busy} onClick={() => props.onRecycleTurn(turn)} size="sm" variant="destructive"><Trash2 className="vk-icon" />{t("memory.data.recycle")}</Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
      {props.hasMore ? <Button disabled={props.busy} onClick={props.onLoadMore} variant="outline">{t("memory.data.loadMore")}</Button> : null}
    </div>
  )
}

// TurnList renders global turn results without exposing raw internal identifiers as titles.
// TurnList 渲染全局回合结果，且不把原始内部标识用作标题。
function TurnList(props: {
  busy: boolean
  turns: ManagementTurn[]
  onOpen: (turn: ManagementTurn) => void
  onRecycle: (turn: ManagementTurn) => void
}) {
  const { t, locale } = useI18n()
  if (props.turns.length === 0) return <DataEmpty description={t("memory.data.noTurns")} />
  return (
    <div className="vk-form-stack">
      {props.turns.map((turn) => (
        <Card key={turn.turn_id}>
          <CardContent className="vk-mem-turn-row">
            <div className="vk-form-fill">
              <div className="vk-mem-clamp2 vk-form-strong">{turn.display_excerpt}</div>
              <div className="vk-form-meta">
                {turn.user ? <span>{turn.user.name}</span> : null}
                {turn.project ? <span>{turn.project.path}</span> : null}
                <span>{formatManagementTime(turn.created_at, locale)}</span>
                <span>{formatBytes(turn.content_bytes)}</span>
                <span>{t("memory.data.memoryCount")}: {turn.derived_memories}</span>
              </div>
            </div>
            <Badge variant="outline">{managementExtractionStatusLabel(t, turn.extraction_status)}</Badge>
            <div className="vk-form-acts">
              <Button disabled={props.busy} onClick={() => props.onOpen(turn)} size="sm" variant="outline">{t("memory.data.details")}</Button>
              <Button aria-label={t("memory.data.recycle")} disabled={props.busy} onClick={() => props.onRecycle(turn)} size="icon" variant="destructive"><Trash2 className="vk-icon" /></Button>
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  )
}

// TurnDetailDialog renders bounded full content and supports explicit incremental loading.
// TurnDetailDialog 渲染有界完整内容并支持显式增量加载。
function TurnDetailDialog(props: {
  busy: boolean
  openTurn: ManagementTurn | null
  content: Record<"user" | "assistant", ManagementTurnContent | null>
  onClose: () => void
  onLoadMore: (section: "user" | "assistant") => Promise<void>
}) {
  const { t, locale } = useI18n()
  return (
    <Dialog open={Boolean(props.openTurn)} onOpenChange={(open) => { if (!open) props.onClose() }}>
      <DialogContent size="xl">
        <DialogHeader>
          <DialogTitle>{t("memory.data.turnDetails")}</DialogTitle>
          <DialogDescription>{props.openTurn?.project?.path ?? formatManagementTime(props.openTurn?.created_at ?? "", locale)}</DialogDescription>
        </DialogHeader>
        {(["user", "assistant"] as const).map((section) => {
          const value = props.content[section]
          return (
            <Card key={section}>
              <CardHeader><CardTitle className="vk-form-sm">{section === "user" ? t("memory.data.userContent") : t("memory.data.assistantContent")}</CardTitle></CardHeader>
              <CardContent className="vk-form-stack">
                {value?.chunk !== undefined ? <pre className="vk-mem-chunk">{value.chunk}</pre> : <StreamPlaceholder className="vk-mem-chunk" label={t("memory.data.loading")} count={5} />}
                {value?.has_more ? <Button disabled={props.busy} onClick={() => void props.onLoadMore(section)} size="sm" variant="outline">{t("memory.data.loadMore")}</Button> : null}
              </CardContent>
            </Card>
          )
        })}
      </DialogContent>
    </Dialog>
  )
}

// RecycleBin renders recoverability and permanent-delete affordances per batch.
// RecycleBin 按批次渲染可恢复状态与永久删除操作。
function RecycleBin(props: {
  busy: boolean
  batches: ManagementRecycleBatch[]
  onRestore: (batchId: string) => void
  onPurge: (batchId: string) => void
}) {
  const { t, locale } = useI18n()
  if (props.batches.length === 0) return <DataEmpty description={t("memory.data.recycleEmpty")} />
  return (
    <div className="vk-form-grid">
      {props.batches.map((batch) => (
        <Card key={batch.batch_id}>
          <CardHeader>
            <div className="vk-mem-tile-head">
              <CardTitle className="vk-form-sm">{batch.reason || t("memory.data.recycleBin")}</CardTitle>
              <Badge variant={batch.restorable ? "default" : "secondary"}>{batch.restorable ? t("memory.data.restorable") : t("memory.data.systemBatch")}</Badge>
            </div>
          </CardHeader>
          <CardContent className="vk-form-stack">
            <div className="vk-form-meta">
              <span>{t("memory.data.turnCount")}: {batch.turn_count}</span>
              <span>{t("memory.data.memoryCount")}: {batch.memory_count}</span>
              <span>{t("memory.data.profileCount")}: {batch.profile_count}</span>
              <span>{formatManagementTime(batch.recycled_at, locale)}</span>
            </div>
            <div className="vk-form-acts">
              {batch.restorable && batch.state === "recycled" ? (
                <Button disabled={props.busy} onClick={() => props.onRestore(batch.batch_id)} size="sm" variant="outline"><RotateCcw className="vk-icon" />{t("memory.data.restore")}</Button>
              ) : null}
              {batch.state === "recycled" ? (
                <Button disabled={props.busy} onClick={() => props.onPurge(batch.batch_id)} size="sm" variant="destructive"><Trash2 className="vk-icon" />{t("memory.data.purge")}</Button>
              ) : null}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  )
}

// ImpactDialog renders the server-authoritative impact and high-risk confirmation.
// ImpactDialog 渲染服务端权威影响与高风险确认。
function ImpactDialog(props: {
  busy: boolean
  impact: ManagementImpact | null
  purgePhrase: string
  onPurgePhraseChange: (value: string) => void
  onClose: () => void
  onConfirm: () => void
}) {
  const { t } = useI18n()
  const impact = props.impact
  // confirmationPhrase is what the user types in their own language before a purge is allowed.
  // confirmationPhrase 是允许永久删除前用户以当前语言输入的短语。
  const confirmationPhrase = t("memory.data.purgeConfirmationPhrase")
  const purgeDescription = formatMessage(t("memory.data.purgeDescription"), { phrase: confirmationPhrase })
  return (
    <Dialog open={Boolean(impact)} onOpenChange={(open) => { if (!open && !props.busy) props.onClose() }}>
      <DialogContent showCloseButton={!props.busy} onEscapeKeyDown={(event) => { if (props.busy) event.preventDefault() }} onPointerDownOutside={(event) => { if (props.busy) event.preventDefault() }}>
        <DialogHeader>
          <DialogTitle>{impact?.action === "purge" ? t("memory.data.purgeTitle") : t("memory.data.impactTitle")}</DialogTitle>
          <DialogDescription>{impact?.action === "purge" ? purgeDescription : t("memory.data.impactDescription")}</DialogDescription>
        </DialogHeader>
        {impact ? (
          <div className="vk-mem-counts">
            <ImpactMetric label={t("memory.data.sessions")} value={impact.session_count} />
            <ImpactMetric label={t("memory.data.turnCount")} value={impact.turn_count} />
            <ImpactMetric label={t("memory.data.memoryCount")} value={impact.memory_count} />
            <ImpactMetric label={t("memory.data.vectorCount")} value={impact.vector_count} />
            <ImpactMetric label={t("memory.data.profileCount")} value={impact.profile_node_count} />
            <ImpactMetric label={t("memory.data.protectedCount")} value={impact.protected_count} danger={impact.protected_count > 0} />
          </div>
        ) : null}
        {impact?.action === "purge" ? (
          <div className="vk-form-stack">
            <Label htmlFor="vmm-purge-confirmation">{purgeDescription}</Label>
            <Input disabled={props.busy} id="vmm-purge-confirmation" onChange={(event) => props.onPurgePhraseChange(event.target.value)} placeholder={confirmationPhrase} value={props.purgePhrase} />
          </div>
        ) : null}
        <DialogFooter>
          <Button disabled={props.busy} onClick={props.onClose} variant="outline">{t("memory.data.cancel")}</Button>
          <Button
            disabled={props.busy || Boolean(impact?.protected_count) || (impact?.action === "purge" && (!impact.required_confirmation || props.purgePhrase !== confirmationPhrase))}
            onClick={props.onConfirm}
            variant={impact?.action === "purge" || impact?.action === "recycle" ? "destructive" : "default"}
          >
            {t("memory.data.confirmAction")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ImpactMetric renders one compact impact count.
// ImpactMetric 渲染一个紧凑影响计数。
function ImpactMetric(props: { label: string; value: number; danger?: boolean }) {
  return (
    <div className="vk-mem-count" data-tone={props.danger ? "bad" : undefined}>
      <span>{props.label}</span>
      <strong className="vk-num">{props.value}</strong>
    </div>
  )
}

// DataEmpty renders one shared empty-management state.
// DataEmpty 渲染一个共享空管理状态。
function DataEmpty(props: { description: string }) {
  const { t } = useI18n()
  return (
    <PageEmptyState description={props.description} icon={<Database aria-hidden="true" />} size="panel" title={t("memory.data.title")} />
  )
}

// managementValue extracts one typed management response from the shared action envelope.
// managementValue 从共享动作信封中提取一个强类型管理响应。
function managementValue<T>(result: VmmMemoryActionResult | null, invalidResponseMessage: string): T | null {
  if (!result) return null
  if (result.data.kind !== "management") throw new Error(invalidResponseMessage)
  return result.data.value as T
}

// managementPage validates the structural fields required by every cursor page before state updates.
// managementPage 在更新状态前校验每个游标分页必需的结构字段。
function managementPage<T>(result: VmmMemoryActionResult | null, invalidResponseMessage: string): ManagementPage<T> | null {
  const page = managementValue<ManagementPage<T>>(result, invalidResponseMessage)
  if (!page) return null
  if (!Array.isArray(page.items) || typeof page.has_more !== "boolean") {
    throw new Error(invalidResponseMessage)
  }
  if (page.next_cursor !== undefined && typeof page.next_cursor !== "string") {
    throw new Error(invalidResponseMessage)
  }
  return page
}

// managementItems validates one collection envelope before any rendering code can iterate over it.
// managementItems 在任何渲染代码迭代前校验列表信封。
function managementItems<T>(result: VmmMemoryActionResult | null, invalidResponseMessage: string): T[] {
  const collection = managementValue<ManagementCollection<T>>(result, invalidResponseMessage)
  if (!collection || !Array.isArray(collection.items)) throw new Error(invalidResponseMessage)
  return collection.items
}

// managementErrorMessage normalizes unknown promise failures into a user-visible message.
// managementErrorMessage 把未知 Promise 失败规范化为用户可见消息。
function managementErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

// managementSessionStatusLabel maps the closed management-session state machine to localized copy.
// managementSessionStatusLabel 将封闭的管理会话状态机映射为本地化文案。
function managementSessionStatusLabel(t: (key: TranslationKey) => string, status: string): string {
  const keys: Record<string, TranslationKey> = {
    active: "memory.data.statusActive",
    archived: "memory.data.statusArchived",
    recycled: "memory.data.statusRecycled",
    forgotten: "memory.data.statusForgotten",
  }
  const key = keys[status]
  return key ? t(key) : status
}

// managementExtractionStatusLabel maps VMM extraction states to localized copy without guessing unknown states.
// managementExtractionStatusLabel 将 VMM 提炼状态映射为本地化文案，并保留未知状态用于诊断。
function managementExtractionStatusLabel(t: (key: TranslationKey) => string, status: string): string {
  const keys: Record<string, TranslationKey> = {
    pending: "memory.data.extractionPending",
    extracted: "memory.data.extractionCompleted",
    failed: "memory.data.extractionFailed",
  }
  const key = keys[status]
  return key ? t(key) : status
}

// managementDateBoundary converts one date-only control value into the user's exact local-day RFC3339 boundary.
// managementDateBoundary 把日期控件值转换为用户本地自然日的准确 RFC3339 边界。
function managementDateBoundary(value: string, boundary: "start" | "end"): string | undefined {
  if (!value) return undefined
  const parts = value.split("-").map(Number)
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part))) return undefined
  const [year, month, day] = parts
  const parsed = boundary === "start"
    ? new Date(year, month - 1, day, 0, 0, 0, 0)
    : new Date(year, month - 1, day, 23, 59, 59, 999)
  return Number.isNaN(parsed.getTime()) ? undefined : parsed.toISOString()
}

// formatManagementTime formats an RFC3339 management timestamp for the current locale.
// formatManagementTime 按当前区域格式化 RFC3339 管理时间戳。
function formatManagementTime(value: string, locale: string): string {
  return formatDateTime(new Date(value), locale) ?? value
}
