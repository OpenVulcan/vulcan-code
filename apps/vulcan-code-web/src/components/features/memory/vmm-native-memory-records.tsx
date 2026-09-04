import * as React from "react"
import { BrainCircuit, Database, Eye, RefreshCw } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { useI18n, type TranslationKey } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmMemorySnapshot,
  VmmNativeMemoryLevelCounts,
  VmmNativeMemoryRecordSummary,
  VmmNativeMemoryRecordsSummary,
  VmmNativeMemorySessionSummary,
  VmmNativeProfileRecordSummary,
  VmmNativeProjectCatalogSummary,
  VmmNativeTurnRecordSummary,
  VmmNativeTurnStatusCounts,
} from "@/types"

// SESSION_OVERVIEW_VALUE is the non-identity selector value used for project-level session summaries.
// SESSION_OVERVIEW_VALUE 是用于项目级会话汇总的非身份选择器值。
const SESSION_OVERVIEW_VALUE = "__native_session_overview__"

// VmmNativeMemoryRecordsProps defines native provenance data and read-only actions.
// VmmNativeMemoryRecordsProps 定义原生来源数据与只读动作。
interface VmmNativeMemoryRecordsProps {
  busy: boolean
  snapshot: VmmMemorySnapshot
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
  ) => Promise<VmmMemoryActionResult | null>
}

// NativeMemoryRecordsRequest is the exact read-only native provenance action payload.
// NativeMemoryRecordsRequest 是准确的只读原生来源动作载荷。
export interface NativeMemoryRecordsRequest extends Record<string, unknown> {
  vmmProjectId: number
  sessionSourceId?: string
}

// NativeRecordCounts contains authoritative status and level totals for one loaded record set.
// NativeRecordCounts 包含一组已加载记录的权威状态与等级汇总。
export interface NativeRecordCounts {
  turnStatuses: VmmNativeTurnStatusCounts
  memoryLevels: VmmNativeMemoryLevelCounts
}

// nativeMemoryRecordsRequest builds one exact project/session provenance request.
// nativeMemoryRecordsRequest 构造一个准确的项目/会话来源请求。
//
// Parameters: projectId is the stable native VMM project id and sessionSourceId is optional.
// 参数：projectId 是稳定原生 VMM 项目标识，sessionSourceId 为可选会话标识。
//
// Returns: a payload that never substitutes a display title for the stable session identity.
// 返回：绝不使用展示标题替代稳定会话身份的载荷。
export function nativeMemoryRecordsRequest(
  projectId: number,
  sessionSourceId?: string | null,
): NativeMemoryRecordsRequest {
  const normalizedSessionId = sessionSourceId?.trim()
  return normalizedSessionId
    ? { vmmProjectId: projectId, sessionSourceId: normalizedSessionId }
    : { vmmProjectId: projectId }
}

// nativeProjectOwnerId selects the current host project's write owner for the initial browser view.
// nativeProjectOwnerId 为初始浏览视图选择当前宿主项目的写入归属。
//
// Parameters: catalog is authoritative and vulcanProjectId is the selected host project tag.
// 参数：catalog 是权威目录，vulcanProjectId 是所选宿主项目标签。
//
// Returns: the bound owner, protected default project, first project, or null when none exist.
// 返回：已绑定归属、受保护默认项目、首个项目，或在无项目时返回空值。
export function nativeProjectOwnerId(
  catalog: VmmNativeProjectCatalogSummary,
  vulcanProjectId?: string | null,
): number | null {
  const binding = vulcanProjectId
    ? catalog.bindings.find((item) => item.vulcanProjectId === vulcanProjectId)
    : undefined
  if (binding) {
    return binding.ownerProjectId
  }
  return catalog.projects.find((project) => project.projectId === 1)?.projectId
    ?? catalog.projects[0]?.projectId
    ?? null
}

// nativePreferredSessionId keeps project overview selected unless the exact preferred session exists.
// nativePreferredSessionId 仅在准确首选会话存在时选中它，否则保持项目概览。
//
// Parameters: sessions are authoritative project sessions and preferredSessionId is a stable host id.
// 参数：sessions 是权威项目会话，preferredSessionId 是稳定宿主标识。
//
// Returns: the normalized matching identity or null for project-level browsing.
// 返回：规范化后的匹配身份；项目级浏览时返回空值。
export function nativePreferredSessionId(
  sessions: VmmNativeMemorySessionSummary[],
  preferredSessionId?: string | null,
): string | null {
  const normalizedPreferred = preferredSessionId?.trim()
  if (!normalizedPreferred) {
    return null
  }
  return sessions.some((session) => session.sessionSourceId === normalizedPreferred)
    ? normalizedPreferred
    : null
}

// nativeRecordCounts aggregates project-level or session-level records without changing their meaning.
// nativeRecordCounts 汇总项目级或会话级记录，且不改变其权威含义。
//
// Parameters: turns and memories are the exact records returned by the native read projection.
// 参数：turns 与 memories 是原生读取投影返回的准确记录。
//
// Returns: counts for every supported turn status and memory level, including zero values.
// 返回：每种受支持回合状态与记忆等级的计数，包括零值。
export function nativeRecordCounts(
  turns: Array<Pick<VmmNativeTurnRecordSummary, "status">>,
  memories: Array<Pick<VmmNativeMemoryRecordSummary, "memoryLevel">>,
): NativeRecordCounts {
  const counts: NativeRecordCounts = {
    turnStatuses: {
      pending: 0,
      completed: 0,
      processedUnknown: 0,
      analysisBypassed: 0,
      corrupted: 0,
    },
    memoryLevels: {
      session: 0,
      phase: 0,
      stable: 0,
      persistent: 0,
    },
  }
  for (const turn of turns) {
    switch (turn.status) {
      case "pending": counts.turnStatuses.pending += 1; break
      case "completed": counts.turnStatuses.completed += 1; break
      case "processed_unknown": counts.turnStatuses.processedUnknown += 1; break
      case "analysis_bypassed": counts.turnStatuses.analysisBypassed += 1; break
      case "corrupted": counts.turnStatuses.corrupted += 1; break
    }
  }
  for (const memory of memories) {
    counts.memoryLevels[memory.memoryLevel] += 1
  }
  return counts
}

// VmmNativeMemoryRecords renders project/session provenance, extraction status, and clickable records.
// VmmNativeMemoryRecords 渲染项目/会话来源、提取状态与可点击记录。
function VmmNativeMemoryRecords({ busy, snapshot, onRunAction }: VmmNativeMemoryRecordsProps) {
  const { t } = useI18n()
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [records, setRecords] = React.useState<VmmNativeMemoryRecordsSummary | null>(null)
  const [projectId, setProjectId] = React.useState("")
  const [sessionId, setSessionId] = React.useState(SESSION_OVERVIEW_VALUE)
  const [selectedTurn, setSelectedTurn] = React.useState<VmmNativeTurnRecordSummary | null>(null)
  const [selectedMemory, setSelectedMemory] = React.useState<VmmNativeMemoryRecordSummary | null>(null)
  const [selectedProfile, setSelectedProfile] = React.useState<VmmNativeProfileRecordSummary | null>(null)
  const [localBusy, setLocalBusy] = React.useState(false)
  const [catalogRefreshVersion, setCatalogRefreshVersion] = React.useState(0)
  const loadedContextRef = React.useRef<string | null>(null)
  const requestSequenceRef = React.useRef(0)
  const effectiveBusy = busy || localBusy

  // loadProjectRecords loads session summaries first, then the preferred exact session when present.
  // loadProjectRecords 先加载会话汇总，再在存在时加载首选准确会话。
  const loadProjectRecords = React.useCallback(async (
    nextProjectId: number,
    preferredSessionId?: string | null,
  ) => {
    const sequence = requestSequenceRef.current + 1
    requestSequenceRef.current = sequence
    setRecords(null)
    setSelectedTurn(null)
    setSelectedMemory(null)
    setSelectedProfile(null)
    setLocalBusy(true)
    try {
      const overviewResult = await onRunAction(
        "native_memory_records",
        nativeMemoryRecordsRequest(nextProjectId),
      )
      if (requestSequenceRef.current !== sequence) {
        return
      }
      const overview = nativeMemoryRecordsValue(overviewResult)
      if (!overview) {
        return
      }
      setRecords(overview)
      const nextSessionId = nativePreferredSessionId(overview.sessions, preferredSessionId)
      if (!nextSessionId) {
        setSessionId(SESSION_OVERVIEW_VALUE)
        return
      }
      const detailResult = await onRunAction(
        "native_memory_records",
        nativeMemoryRecordsRequest(nextProjectId, nextSessionId),
      )
      if (requestSequenceRef.current !== sequence) {
        return
      }
      const detail = nativeMemoryRecordsValue(detailResult)
      if (detail) {
        setSessionId(nextSessionId)
        setRecords(detail)
      }
    } finally {
      if (requestSequenceRef.current === sequence) {
        setLocalBusy(false)
      }
    }
  }, [onRunAction])

  // loadSessionRecords loads one exact session or returns to the project-level overview.
  // loadSessionRecords 加载一个准确会话，或返回项目级概览。
  const loadSessionRecords = React.useCallback(async (nextSessionId: string) => {
    const numericProjectId = Number(projectId)
    if (!Number.isSafeInteger(numericProjectId) || numericProjectId <= 0) {
      return
    }
    const sequence = requestSequenceRef.current + 1
    requestSequenceRef.current = sequence
    setSelectedTurn(null)
    setSelectedMemory(null)
    setSelectedProfile(null)
    setLocalBusy(true)
    try {
      const result = await onRunAction(
        "native_memory_records",
        nativeMemoryRecordsRequest(
          numericProjectId,
          nextSessionId === SESSION_OVERVIEW_VALUE ? null : nextSessionId,
        ),
      )
      if (requestSequenceRef.current !== sequence) {
        return
      }
      const nextRecords = nativeMemoryRecordsValue(result)
      if (nextRecords) {
        setSessionId(nextSessionId)
        setRecords(nextRecords)
      }
    } finally {
      if (requestSequenceRef.current === sequence) {
        setLocalBusy(false)
      }
    }
  }, [onRunAction, projectId])

  React.useEffect(() => {
    const contextKey = `${snapshot.selectedProjectId ?? ""}\u0000${snapshot.selectedConversationId ?? ""}`
    if (loadedContextRef.current === contextKey || busy) {
      return
    }
    loadedContextRef.current = contextKey
    setLocalBusy(true)
    void onRunAction("native_project_catalog").then(async (result) => {
      const nextCatalog = nativeProjectCatalogValue(result)
      if (!nextCatalog) {
        loadedContextRef.current = null
        return
      }
      setCatalog(nextCatalog)
      const ownerId = nativeProjectOwnerId(nextCatalog, snapshot.selectedProjectId)
      if (ownerId === null) {
        setProjectId("")
        setRecords(null)
        return
      }
      setProjectId(String(ownerId))
      await loadProjectRecords(ownerId, snapshot.selectedConversationId)
    }).finally(() => {
      setLocalBusy(false)
    })
  }, [busy, catalogRefreshVersion, loadProjectRecords, onRunAction, snapshot.selectedConversationId, snapshot.selectedProjectId])

  const selectedSession = records?.sessions.find(
    (session) => session.sessionSourceId === records.selectedSessionSourceId,
  ) ?? null
  const selectedProject = catalog?.projects.find(
    (project) => String(project.projectId) === projectId,
  ) ?? null
  const projectOverview = records !== null && records.selectedSessionSourceId == null
  const recordCounts = React.useMemo(
    () => nativeRecordCounts(records?.turns ?? [], records?.memories ?? []),
    [records],
  )

  return (
    <Card>
      <CardHeader className="gap-1">
        <CardTitle className="flex items-center gap-2 text-base">
          <Database className="size-4" aria-hidden="true" />
          {t("memory.nativeRecords.title")}
        </CardTitle>
        <CardDescription>{t("memory.nativeRecords.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
          <div className="space-y-2">
            <Label>{t("memory.nativeRecords.project")}</Label>
            <Select
              disabled={effectiveBusy || !catalog?.projects.length}
              onValueChange={(value) => {
                setProjectId(value)
                setSessionId(SESSION_OVERVIEW_VALUE)
                void loadProjectRecords(Number(value), null)
              }}
              value={projectId}
            >
              <SelectTrigger><SelectValue placeholder={t("memory.nativeRecords.selectProject")} /></SelectTrigger>
              <SelectContent>
                {catalog?.projects.map((project) => (
                  <SelectItem key={project.projectId} value={String(project.projectId)}>
                    {project.name} · {project.tag}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label>{t("memory.nativeRecords.session")}</Label>
            <Select
              disabled={effectiveBusy || !records?.sessions.length}
              onValueChange={(value) => void loadSessionRecords(value)}
              value={sessionId}
            >
              <SelectTrigger><SelectValue placeholder={t("memory.nativeRecords.selectSession")} /></SelectTrigger>
              <SelectContent>
                <SelectItem value={SESSION_OVERVIEW_VALUE}>{t("memory.nativeRecords.sessionOverview")}</SelectItem>
                {records?.sessions.map((session) => (
                  <SelectItem key={session.sessionSourceId} value={session.sessionSourceId}>
                    {nativeSessionLabel(session)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button
            disabled={effectiveBusy}
            onClick={() => {
              if (!projectId) {
                loadedContextRef.current = null
                setCatalogRefreshVersion((current) => current + 1)
                return
              }
              const selected = sessionId === SESSION_OVERVIEW_VALUE ? null : sessionId
              if (selected) {
                void loadSessionRecords(selected)
              } else {
                void loadProjectRecords(Number(projectId), null)
              }
            }}
            type="button"
            variant="outline"
          >
            <RefreshCw className={`size-4 ${effectiveBusy ? "animate-spin" : ""}`} aria-hidden="true" />
            {t("memory.nativeRecords.refresh")}
          </Button>
        </div>

        {!catalog?.projects.length && !effectiveBusy ? (
          <NativeRecordsEmpty
            description={t("memory.nativeRecords.noProjects")}
            title={t("memory.nativeRecords.title")}
          />
        ) : null}

        {records
          && records.sessions.length === 0
          && records.turns.length === 0
          && records.memories.length === 0
          && records.profileNodes.length === 0
          && !effectiveBusy ? (
          <NativeRecordsEmpty
            description={t("memory.nativeRecords.noProjectRecords")}
            title={t("memory.nativeRecords.title")}
          />
        ) : null}

        {records && projectOverview ? (
          <NativeProjectSummary
            counts={recordCounts}
            memoryCount={records.memories.length}
            profileNodeCount={records.profileNodes.length}
            projectName={selectedProject?.name ?? String(records.projectId)}
            sessionCount={records.sessions.length}
            turnCount={records.turns.length}
          />
        ) : null}

        {records && projectOverview && records.sessions.length > 0 ? (
          <section className="space-y-3">
            <h3 className="text-sm font-medium">{t("memory.nativeRecords.sessions")}</h3>
            <div className="grid gap-3 md:grid-cols-2">
              {records.sessions.map((session) => (
                <button
                  className="rounded-xl border p-4 text-left transition-colors hover:bg-muted/40"
                  key={session.sessionSourceId}
                  onClick={() => void loadSessionRecords(session.sessionSourceId)}
                  type="button"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate font-medium">{nativeSessionLabel(session)}</p>
                      <p className="truncate font-mono text-xs text-muted-foreground">{session.sessionSourceId}</p>
                    </div>
                    <Eye className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <span>{t("memory.nativeRecords.turns")} {session.turnCount}</span>
                    <span>{t("memory.nativeRecords.memories")} {session.memoryCount}</span>
                    <span>{t("memory.nativeRecords.profileNodes")} {session.profileNodeCount}</span>
                    <span>{t("memory.nativeRecords.completed")} {session.turnStatuses.completed}</span>
                    <span>{t("memory.nativeRecords.pending")} {session.turnStatuses.pending}</span>
                  </div>
                  {session.lastActivityAtUnixMs != null ? (
                    <p className="mt-2 text-xs text-muted-foreground">
                      {t("memory.nativeRecords.lastActivity")} {formatNativeTimestamp(session.lastActivityAtUnixMs)}
                    </p>
                  ) : null}
                </button>
              ))}
            </div>
          </section>
        ) : null}

        {records && selectedSession ? <NativeSessionSummary session={selectedSession} /> : null}

        {records ? (
          <>
            <section className="space-y-3">
              <h3 className="text-sm font-medium">
                {t(projectOverview
                  ? "memory.nativeRecords.projectTurnRecords"
                  : "memory.nativeRecords.turnRecords")}
              </h3>
              {records.turns.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t("memory.nativeRecords.noTurns")}</p>
              ) : (
                <div className="space-y-2">
                  {records.turns.map((turn) => (
                    <button
                      className="w-full rounded-xl border p-4 text-left transition-colors hover:bg-muted/40"
                      key={turn.turnId}
                      onClick={() => setSelectedTurn(turn)}
                      type="button"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <Badge variant={turn.status === "completed" ? "default" : "secondary"}>
                            {t(nativeTurnStatusKey(turn.status))}
                          </Badge>
                          <span className="font-mono text-xs text-muted-foreground">#{turn.turnId}</span>
                          <span className="text-xs text-muted-foreground">
                            {formatNativeTimestamp(turn.createdAtUnixMs)}
                          </span>
                        </div>
                        <span className="text-xs text-muted-foreground">
                          {t("memory.nativeRecords.generatedMemories")} {turn.generatedMemoryIds.length}
                          {" · "}
                          {t("memory.nativeRecords.generatedProfiles")} {turn.generatedProfileNodeIds.length}
                        </span>
                      </div>
                      <p className="mt-3 line-clamp-2 text-sm">{turn.userPreview || t("memory.nativeRecords.noUserPreview")}</p>
                      {projectOverview ? (
                        <p className="mt-2 truncate text-xs text-muted-foreground">
                          {t("memory.nativeRecords.session")} {nativeSessionReference(records.sessions, turn.sessionSourceId)}
                        </p>
                      ) : null}
                      {turn.analysisAttemptCount > 0 ? (
                        <p className="mt-2 text-xs text-amber-700 dark:text-amber-300">
                          {t("memory.nativeRecords.retryCount")} {turn.analysisAttemptCount}
                          {turn.analysisLastError ? ` · ${turn.analysisLastError}` : ""}
                        </p>
                      ) : null}
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="space-y-3">
              <h3 className="text-sm font-medium">
                {t(projectOverview
                  ? "memory.nativeRecords.projectMemories"
                  : "memory.nativeRecords.sessionMemories")}
              </h3>
              {records.memories.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t("memory.nativeRecords.noMemories")}</p>
              ) : (
                <div className="grid gap-3 md:grid-cols-2">
                  {records.memories.map((memory) => (
                    <button
                      className="rounded-xl border p-4 text-left transition-colors hover:bg-muted/40"
                      key={memory.memoryId}
                      onClick={() => setSelectedMemory(memory)}
                      type="button"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <p className="line-clamp-2 font-medium">{memory.summary}</p>
                        <div className="flex shrink-0 gap-1">
                          <Badge variant="outline">{t(nativeMemoryLevelKey(memory.memoryLevel))}</Badge>
                          <Badge variant={memory.state === "active" ? "secondary" : "outline"}>
                            {t(nativeMemoryStateKey(memory.state))}
                          </Badge>
                        </div>
                      </div>
                      <p className="mt-2 line-clamp-3 text-sm text-muted-foreground">{memory.content}</p>
                      <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                        <span>#{memory.memoryId}</span>
                        <span>{t(nativeMemorySourceKey(memory.sourceKind))}</span>
                        {memory.sourceTurnId != null ? <span>{t("memory.nativeRecords.sourceTurn")} #{memory.sourceTurnId}</span> : null}
                        {projectOverview ? (
                          <span>
                            {memory.sessionSourceId
                              ? nativeSessionReference(records.sessions, memory.sessionSourceId)
                              : t("memory.nativeRecords.projectOnly")}
                          </span>
                        ) : null}
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="space-y-3">
              <h3 className="text-sm font-medium">{t("memory.nativeRecords.profileNodes")}</h3>
              {records.profileNodes.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t("memory.nativeRecords.noProfileNodes")}</p>
              ) : (
                <div className="grid gap-3 md:grid-cols-2">
                  {records.profileNodes.map((profile) => (
                    <button
                      className="rounded-xl border p-4 text-left transition-colors hover:bg-muted/40"
                      key={`${profile.target}-${profile.profileNodeId}`}
                      onClick={() => setSelectedProfile(profile)}
                      type="button"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <p className="line-clamp-3 font-medium">{profile.content}</p>
                        <div className="flex shrink-0 gap-1">
                          <Badge variant="outline">
                            {t(profile.target === "user" ? "memory.profile.user" : "memory.profile.project")}
                          </Badge>
                          <Badge variant={profile.effectiveState === "active" ? "secondary" : "outline"}>
                            {t(nativeProfileStateKey(profile.effectiveState))}
                          </Badge>
                        </div>
                      </div>
                      <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                        <span>#{profile.profileNodeId}</span>
                        <span>{t(nativeProfileSourceKey(profile.sourceKind))}</span>
                        <span>
                          {t("memory.profile.sourceTurns")} {profile.sourceTurnIds.map((id) => `#${id}`).join(", ")}
                        </span>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </section>
          </>
        ) : null}
      </CardContent>

      <NativeTurnSheet
        memories={records?.memories ?? []}
        profiles={records?.profileNodes ?? []}
        onClose={() => setSelectedTurn(null)}
        onOpenMemory={(memory) => {
          setSelectedTurn(null)
          setSelectedMemory(memory)
        }}
        onOpenProfile={(profile) => {
          setSelectedTurn(null)
          setSelectedProfile(profile)
        }}
        turn={selectedTurn}
      />
      <NativeMemorySheet memory={selectedMemory} onClose={() => setSelectedMemory(null)} />
      <NativeProfileSheet profile={selectedProfile} onClose={() => setSelectedProfile(null)} />
    </Card>
  )
}

// NativeProjectSummary renders complete project-wide turn states and memory-level totals.
// NativeProjectSummary 渲染完整的项目级回合状态与记忆等级汇总。
function NativeProjectSummary({
  projectName,
  sessionCount,
  turnCount,
  memoryCount,
  profileNodeCount,
  counts,
}: {
  projectName: string
  sessionCount: number
  turnCount: number
  memoryCount: number
  profileNodeCount: number
  counts: NativeRecordCounts
}) {
  const { t } = useI18n()
  return (
    <section className="space-y-3 rounded-xl border bg-muted/20 p-4">
      <div>
        <h3 className="font-medium">{projectName}</h3>
        <p className="text-xs text-muted-foreground">{t("memory.nativeRecords.projectSummary")}</p>
      </div>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <NativeCount label={t("memory.nativeRecords.sessions")} value={sessionCount} />
        <NativeCount label={t("memory.nativeRecords.turns")} value={turnCount} />
        <NativeCount label={t("memory.nativeRecords.memories")} value={memoryCount} />
        <NativeCount label={t("memory.nativeRecords.profileNodes")} value={profileNodeCount} />
      </div>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
        <NativeCount label={t("memory.nativeRecords.completed")} value={counts.turnStatuses.completed} />
        <NativeCount label={t("memory.nativeRecords.pending")} value={counts.turnStatuses.pending} />
        <NativeCount label={t("memory.nativeRecords.processedUnknown")} value={counts.turnStatuses.processedUnknown} />
        <NativeCount label={t("memory.nativeRecords.bypassed")} value={counts.turnStatuses.analysisBypassed} />
        <NativeCount label={t("memory.nativeRecords.corrupted")} value={counts.turnStatuses.corrupted} />
      </div>
      <NativeMemoryLevelCountsView counts={counts.memoryLevels} />
    </section>
  )
}

// NativeSessionSummary renders authoritative turn states and memory-level totals.
// NativeSessionSummary 渲染权威回合状态与记忆等级汇总。
function NativeSessionSummary({ session }: { session: VmmNativeMemorySessionSummary }) {
  const { t } = useI18n()
  return (
    <section className="space-y-3 rounded-xl border bg-muted/20 p-4">
      <div>
        <h3 className="font-medium">{nativeSessionLabel(session)}</h3>
        <p className="font-mono text-xs text-muted-foreground">{session.sessionSourceId}</p>
      </div>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
        <NativeCount label={t("memory.nativeRecords.completed")} value={session.turnStatuses.completed} />
        <NativeCount label={t("memory.nativeRecords.pending")} value={session.turnStatuses.pending} />
        <NativeCount label={t("memory.nativeRecords.processedUnknown")} value={session.turnStatuses.processedUnknown} />
        <NativeCount label={t("memory.nativeRecords.bypassed")} value={session.turnStatuses.analysisBypassed} />
        <NativeCount label={t("memory.nativeRecords.corrupted")} value={session.turnStatuses.corrupted} />
      </div>
      <div className="grid gap-2 sm:grid-cols-3">
        <NativeCount label={t("memory.nativeRecords.turns")} value={session.turnCount} />
        <NativeCount label={t("memory.nativeRecords.memories")} value={session.memoryCount} />
        <NativeCount label={t("memory.nativeRecords.profileNodes")} value={session.profileNodeCount} />
      </div>
      <NativeMemoryLevelCountsView counts={session.memoryLevels} />
    </section>
  )
}

// NativeMemoryLevelCountsView renders all four lifecycle levels without hiding zero counts.
// NativeMemoryLevelCountsView 渲染全部四个生命周期等级且不隐藏零计数。
function NativeMemoryLevelCountsView({ counts }: { counts: VmmNativeMemoryLevelCounts }) {
  const { t } = useI18n()
  return (
    <div className="flex flex-wrap gap-2">
      <Badge variant="outline">{t("memory.nativeRecords.levelSession")} {counts.session}</Badge>
      <Badge variant="outline">{t("memory.nativeRecords.levelPhase")} {counts.phase}</Badge>
      <Badge variant="outline">{t("memory.nativeRecords.levelStable")} {counts.stable}</Badge>
      <Badge variant="outline">{t("memory.nativeRecords.levelPersistent")} {counts.persistent}</Badge>
    </div>
  )
}

// NativeCount renders one compact labeled count.
// NativeCount 渲染一个紧凑的带标签计数。
function NativeCount({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg border bg-background px-3 py-2">
      <p className="text-xs text-muted-foreground">{label}</p>
      <p className="mt-1 text-lg font-semibold">{value}</p>
    </div>
  )
}

// NativeRecordsEmpty renders one native browser empty state.
// NativeRecordsEmpty 渲染一个原生浏览器空状态。
function NativeRecordsEmpty({ title, description }: { title: string; description: string }) {
  return (
    <Empty className="min-h-40" variant="outlined">
      <EmptyHeader>
        <EmptyMedia variant="icon"><BrainCircuit className="size-5" /></EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
    </Empty>
  )
}

// NativeTurnSheet shows the exact persisted timeline and extraction diagnostics for one turn.
// NativeTurnSheet 展示一个回合的准确持久时间线与提取诊断。
function NativeTurnSheet({
  turn,
  memories,
  profiles,
  onClose,
  onOpenMemory,
  onOpenProfile,
}: {
  turn: VmmNativeTurnRecordSummary | null
  memories: VmmNativeMemoryRecordSummary[]
  profiles: VmmNativeProfileRecordSummary[]
  onClose: () => void
  onOpenMemory: (memory: VmmNativeMemoryRecordSummary) => void
  onOpenProfile: (profile: VmmNativeProfileRecordSummary) => void
}) {
  const { t } = useI18n()
  const generatedMemories = turn
    ? memories.filter((memory) => turn.generatedMemoryIds.includes(memory.memoryId))
    : []
  const generatedProfiles = turn
    ? profiles.filter((profile) => turn.generatedProfileNodeIds.includes(profile.profileNodeId))
    : []
  return (
    <Sheet open={turn !== null} onOpenChange={(open) => { if (!open) onClose() }}>
      <SheetContent className="overflow-y-auto" size="md">
        <SheetHeader>
          <SheetTitle>{t("memory.nativeRecords.turn")} #{turn?.turnId}</SheetTitle>
          <SheetDescription>
            {turn ? `${t(nativeTurnStatusKey(turn.status))} · ${formatNativeTimestamp(turn.createdAtUnixMs)}` : ""}
          </SheetDescription>
        </SheetHeader>
        <div className="space-y-4 px-4 pb-4">
          {turn?.analysisDetails ? (
            <NativeDetail label={t("memory.nativeRecords.analysisDetails")} value={turn.analysisDetails} />
          ) : null}
          {turn?.analysisLastError ? (
            <NativeDetail label={t("memory.nativeRecords.lastError")} value={turn.analysisLastError} />
          ) : null}
          <section className="space-y-2">
            <h3 className="text-sm font-medium">{t("memory.nativeRecords.generatedMemories")}</h3>
            {generatedMemories.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t("memory.nativeRecords.noGeneratedMemories")}</p>
            ) : (
              <div className="space-y-2">
                {generatedMemories.map((memory) => (
                  <button
                    className="flex w-full items-start justify-between gap-3 rounded-lg border p-3 text-left transition-colors hover:bg-muted/40"
                    key={memory.memoryId}
                    onClick={() => onOpenMemory(memory)}
                    type="button"
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-medium">{memory.summary}</span>
                      <span className="mt-1 block text-xs text-muted-foreground">#{memory.memoryId}</span>
                    </span>
                    <Badge variant="outline">{t(nativeMemoryLevelKey(memory.memoryLevel))}</Badge>
                  </button>
                ))}
              </div>
            )}
          </section>
          <section className="space-y-2">
            <h3 className="text-sm font-medium">{t("memory.nativeRecords.generatedProfiles")}</h3>
            {generatedProfiles.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t("memory.nativeRecords.noGeneratedProfiles")}</p>
            ) : (
              <div className="space-y-2">
                {generatedProfiles.map((profile) => (
                  <button
                    className="flex w-full items-start justify-between gap-3 rounded-lg border p-3 text-left transition-colors hover:bg-muted/40"
                    key={`${profile.target}-${profile.profileNodeId}`}
                    onClick={() => onOpenProfile(profile)}
                    type="button"
                  >
                    <span className="min-w-0">
                      <span className="block line-clamp-2 text-sm font-medium">{profile.content}</span>
                      <span className="mt-1 block text-xs text-muted-foreground">#{profile.profileNodeId}</span>
                    </span>
                    <Badge variant="outline">
                      {t(profile.target === "user" ? "memory.profile.user" : "memory.profile.project")}
                    </Badge>
                  </button>
                ))}
              </div>
            )}
          </section>
          <section className="space-y-2">
            <h3 className="text-sm font-medium">{t("memory.nativeRecords.timeline")}</h3>
            {turn?.timeline.map((item, index) => (
              <div className="rounded-lg border p-3" key={`${item.occurredAtUnixMs}-${index}`}>
                <div className="mb-2 flex items-center justify-between gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">{t(nativeTimelineRoleKey(item.role))}</Badge>
                  <span>{formatNativeTimestamp(item.occurredAtUnixMs)}</span>
                </div>
                <p className="whitespace-pre-wrap text-sm">{item.content}</p>
              </div>
            ))}
          </section>
        </div>
      </SheetContent>
    </Sheet>
  )
}

// NativeMemorySheet shows one complete durable memory and its provenance identifiers.
// NativeMemorySheet 展示一条完整持久记忆及其来源标识。
function NativeMemorySheet({
  memory,
  onClose,
}: {
  memory: VmmNativeMemoryRecordSummary | null
  onClose: () => void
}) {
  const { t } = useI18n()
  return (
    <Sheet open={memory !== null} onOpenChange={(open) => { if (!open) onClose() }}>
      <SheetContent className="overflow-y-auto" size="md">
        <SheetHeader>
          <SheetTitle>{memory?.summary}</SheetTitle>
          <SheetDescription>
            {memory ? `#${memory.memoryId} · ${formatNativeTimestamp(memory.createdAtUnixMs)}` : ""}
          </SheetDescription>
        </SheetHeader>
        <div className="space-y-4 px-4 pb-4">
          {memory ? (
            <div className="flex flex-wrap gap-2">
              <Badge>{t(nativeMemoryLevelKey(memory.memoryLevel))}</Badge>
              <Badge variant="secondary">{t(nativeMemoryStateKey(memory.state))}</Badge>
              <Badge variant="outline">{t(nativeMemorySourceKey(memory.sourceKind))}</Badge>
              <Badge variant="outline">{t(nativeMemoryCategoryKey(memory.category))}</Badge>
            </div>
          ) : null}
          {memory ? <NativeDetail label={t("memory.content.fullDetails")} value={memory.content} /> : null}
          {memory?.sourceTurnId != null ? (
            <NativeDetail label={t("memory.nativeRecords.sourceTurn")} value={`#${memory.sourceTurnId}`} />
          ) : null}
          {memory?.sessionSourceId ? (
            <NativeDetail label={t("memory.nativeRecords.session")} value={memory.sessionSourceId} />
          ) : null}
          {memory ? (
            <NativeDetail label={t("memory.nativeRecords.priority")} value={String(memory.priority)} />
          ) : null}
          {memory?.tags.length ? (
            <NativeDetail label={t("memory.nativeRecords.tags")} value={memory.tags.join(", ")} />
          ) : null}
          {memory?.statusReason ? (
            <NativeDetail label={t("memory.nativeRecords.statusReason")} value={memory.statusReason} />
          ) : null}
          {memory?.expiresAtUnixMs != null ? (
            <NativeDetail
              label={t("memory.nativeRecords.expiresAt")}
              value={formatNativeTimestamp(memory.expiresAtUnixMs)}
            />
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  )
}

// NativeProfileSheet shows one complete user or project profile node and its turn provenance.
// NativeProfileSheet 显示一条完整的用户或项目画像节点及其回合来源。
function NativeProfileSheet({
  profile,
  onClose,
}: {
  profile: VmmNativeProfileRecordSummary | null
  onClose: () => void
}) {
  const { t } = useI18n()
  return (
    <Sheet open={profile !== null} onOpenChange={(open) => { if (!open) onClose() }}>
      <SheetContent className="overflow-y-auto" size="md">
        <SheetHeader>
          <SheetTitle>
            {t(profile?.target === "user" ? "memory.profile.user" : "memory.profile.project")}
            {profile ? ` #${profile.profileNodeId}` : ""}
          </SheetTitle>
          <SheetDescription>
            {profile ? formatNativeTimestamp(profile.updatedAtUnixMs) : ""}
          </SheetDescription>
        </SheetHeader>
        <div className="space-y-4 px-4 pb-4">
          {profile ? (
            <div className="flex flex-wrap gap-2">
              <Badge>{t(nativeProfileStateKey(profile.effectiveState))}</Badge>
              <Badge variant="secondary">
                {t("memory.profile.level")}{t("global.labelSeparator")}{profile.level}
              </Badge>
              <Badge variant="outline">{t(nativeProfileSourceKey(profile.sourceKind))}</Badge>
              {!profile.activeForInjection ? (
                <Badge variant="outline">{t("memory.profile.notInjected")}</Badge>
              ) : null}
            </div>
          ) : null}
          {profile ? <NativeDetail label={t("memory.content.fullDetails")} value={profile.content} /> : null}
          {profile?.sourceTurnIds.length ? (
            <NativeDetail
              label={t("memory.profile.sourceTurns")}
              value={profile.sourceTurnIds.map((turnId) => `#${turnId}`).join(", ")}
            />
          ) : null}
          {profile?.sourceId ? (
            <NativeDetail label={t("memory.profile.source")} value={profile.sourceId} />
          ) : null}
          {profile ? (
            <NativeDetail label={t("memory.profile.priority")} value={String(profile.priority)} />
          ) : null}
          {profile?.statusReason ? (
            <NativeDetail label={t("memory.profile.statusReason")} value={profile.statusReason} />
          ) : null}
          {profile?.expiresAtUnixMs != null ? (
            <NativeDetail
              label={t("memory.nativeRecords.expiresAt")}
              value={formatNativeTimestamp(profile.expiresAtUnixMs)}
            />
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  )
}

// NativeDetail renders one selectable detail field.
// NativeDetail 渲染一个可选择的详情字段。
function NativeDetail({ label, value }: { label: string; value: string }) {
  return (
    <section className="space-y-1.5">
      <h3 className="text-sm font-medium">{label}</h3>
      <p className="whitespace-pre-wrap rounded-lg border bg-muted/30 p-3 text-sm">{value}</p>
    </section>
  )
}

// nativeProjectCatalogValue narrows one action result to the native project catalog.
// nativeProjectCatalogValue 将一个动作结果收窄为原生项目目录。
function nativeProjectCatalogValue(
  result: VmmMemoryActionResult | null,
): VmmNativeProjectCatalogSummary | null {
  return result?.data.kind === "native_project_catalog" ? result.data.value : null
}

// nativeMemoryRecordsValue narrows one action result to native provenance records.
// nativeMemoryRecordsValue 将一个动作结果收窄为原生来源记录。
function nativeMemoryRecordsValue(
  result: VmmMemoryActionResult | null,
): VmmNativeMemoryRecordsSummary | null {
  return result?.data.kind === "native_memory_records" ? result.data.value : null
}

// nativeSessionLabel returns the optional host title while keeping the stable id separately visible.
// nativeSessionLabel 返回可选宿主标题，同时让稳定标识保持独立可见。
function nativeSessionLabel(session: VmmNativeMemorySessionSummary): string {
  return session.displayTitle?.trim() || session.sessionSourceId
}

// nativeSessionReference combines the optional host title with the stable session identity.
// nativeSessionReference 将可选宿主标题与稳定会话身份组合为展示引用。
function nativeSessionReference(
  sessions: VmmNativeMemorySessionSummary[],
  sessionSourceId: string,
): string {
  const session = sessions.find((item) => item.sessionSourceId === sessionSourceId)
  if (!session?.displayTitle?.trim()) {
    return sessionSourceId
  }
  return `${session.displayTitle.trim()} · ${sessionSourceId}`
}

// formatNativeTimestamp formats one JavaScript-safe Unix millisecond value for the current locale.
// formatNativeTimestamp 将一个 JavaScript 安全的 Unix 毫秒值按当前区域格式化。
function formatNativeTimestamp(timestampMs: number): string {
  return new Date(timestampMs).toLocaleString()
}

// nativeTurnStatusKey maps one stable turn state to localized display text.
// nativeTurnStatusKey 将一个稳定回合状态映射为本地化展示文本。
function nativeTurnStatusKey(status: VmmNativeTurnRecordSummary["status"]): TranslationKey {
  switch (status) {
    case "pending": return "memory.nativeRecords.statusPending"
    case "completed": return "memory.nativeRecords.statusCompleted"
    case "processed_unknown": return "memory.nativeRecords.statusProcessedUnknown"
    case "analysis_bypassed": return "memory.nativeRecords.statusBypassed"
    case "corrupted": return "memory.nativeRecords.statusCorrupted"
  }
}

// nativeMemoryLevelKey maps one stable memory level to localized display text.
// nativeMemoryLevelKey 将一个稳定记忆等级映射为本地化展示文本。
function nativeMemoryLevelKey(level: VmmNativeMemoryRecordSummary["memoryLevel"]): TranslationKey {
  switch (level) {
    case "session": return "memory.nativeRecords.levelSession"
    case "phase": return "memory.nativeRecords.levelPhase"
    case "stable": return "memory.nativeRecords.levelStable"
    case "persistent": return "memory.nativeRecords.levelPersistent"
  }
}

// nativeMemoryStateKey maps one stable memory lifecycle state to localized display text.
// nativeMemoryStateKey 将一个稳定记忆生命周期状态映射为本地化展示文本。
function nativeMemoryStateKey(state: VmmNativeMemoryRecordSummary["state"]): TranslationKey {
  switch (state) {
    case "active": return "memory.nativeRecords.stateActive"
    case "superseded": return "memory.nativeRecords.stateSuperseded"
    case "deleted": return "memory.nativeRecords.stateDeleted"
    case "expired": return "memory.nativeRecords.stateExpired"
    case "recycled": return "memory.nativeRecords.stateRecycled"
  }
}

// nativeMemorySourceKey maps one stable memory source to localized display text.
// nativeMemorySourceKey 将一个稳定记忆来源映射为本地化展示文本。
function nativeMemorySourceKey(source: VmmNativeMemoryRecordSummary["sourceKind"]): TranslationKey {
  switch (source) {
    case "turn": return "memory.nativeRecords.sourceTurnExtracted"
    case "direct": return "memory.nativeRecords.sourceDirect"
    case "import": return "memory.nativeRecords.sourceImport"
    case "system_seed": return "memory.nativeRecords.sourceSystemSeed"
    case "legacy": return "memory.nativeRecords.sourceLegacy"
  }
}

// nativeMemoryCategoryKey maps one authoritative memory category to localized display text.
// nativeMemoryCategoryKey 将一个权威记忆分类映射为本地化展示文本。
function nativeMemoryCategoryKey(category: VmmNativeMemoryRecordSummary["category"]): TranslationKey {
  switch (category) {
    case "general": return "memory.nativeRecords.categoryGeneral"
    case "architecture_decision": return "memory.nativeRecords.categoryArchitecture"
    case "tech_spec_api": return "memory.nativeRecords.categoryTechSpec"
    case "business_logic": return "memory.nativeRecords.categoryBusiness"
    case "requirement_todo": return "memory.nativeRecords.categoryRequirement"
    case "project_context": return "memory.nativeRecords.categoryProject"
    case "logical_bug_debt": return "memory.nativeRecords.categoryBugDebt"
    case "security_policy": return "memory.nativeRecords.categorySecurity"
  }
}

// nativeProfileStateKey maps one closed native profile state to localized user text.
// nativeProfileStateKey 将一个封闭的原生画像状态映射为本地化用户文本。
function nativeProfileStateKey(
  state: VmmNativeProfileRecordSummary["effectiveState"]
    | VmmNativeProfileRecordSummary["storedState"],
): TranslationKey {
  switch (state) {
    case "active": return "memory.profile.stateActive"
    case "pending": return "memory.profile.statePending"
    case "scheduled": return "memory.profile.stateScheduled"
    case "inactive": return "memory.profile.stateInactive"
    case "expired": return "memory.profile.stateExpired"
  }
}

// nativeProfileSourceKey maps one closed native profile source to localized user text.
// nativeProfileSourceKey 将一个封闭的原生画像来源映射为本地化用户文本。
function nativeProfileSourceKey(
  sourceKind: VmmNativeProfileRecordSummary["sourceKind"],
): TranslationKey {
  switch (sourceKind) {
    case "turn_extract": return "memory.profile.sourceTurnExtract"
    case "manual_instruction": return "memory.profile.sourceManualInstruction"
    case "system_seed": return "memory.profile.sourceSystemSeed"
    case "retained_after_subject_delete": return "memory.profile.sourceRetained"
  }
}

// nativeTimelineRoleKey maps one stable timeline role to localized display text.
// nativeTimelineRoleKey 将一个稳定时间线角色映射为本地化展示文本。
function nativeTimelineRoleKey(
  role: VmmNativeTurnRecordSummary["timeline"][number]["role"],
): TranslationKey {
  switch (role) {
    case "user": return "memory.nativeRecords.roleUser"
    case "assistant": return "memory.nativeRecords.roleAssistant"
    case "system": return "memory.nativeRecords.roleSystem"
    case "tool": return "memory.nativeRecords.roleTool"
  }
}

export { VmmNativeMemoryRecords }
export type { VmmNativeMemoryRecordsProps }
