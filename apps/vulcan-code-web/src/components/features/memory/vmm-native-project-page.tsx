import * as React from "react"
import {
  CircleAlert,
  DatabaseZap,
  Link2,
  LoaderCircle,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { useI18n, type TranslationKey } from "@/i18n"
import type {
  VmmMemoryActionResult,
  VmmNativeClearPreview,
  VmmNativeMigrationPreview,
  VmmNativeProjectCatalogSummary,
  VmmNativeProjectDeletePreview,
  VmmProjectScopeSummary,
} from "@/types"

import type { MemoryActionRunOptions } from "./use-memory-area-controller"

// VmmNativeProjectPageProps defines native project catalog data and guarded maintenance actions.
// VmmNativeProjectPageProps 定义原生项目目录数据与受保护维护动作。
interface VmmNativeProjectPageProps {
  busy: boolean
  notice: string
  projects: VmmProjectScopeSummary[]
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
    options?: MemoryActionRunOptions,
  ) => Promise<VmmMemoryActionResult | null>
}

// NativeProjectPreview is the only destructive preview currently awaiting confirmation.
// NativeProjectPreview 是当前唯一等待确认的破坏性预检。
export type NativeProjectPreview =
  | { commandId: string; kind: "migration"; value: VmmNativeMigrationPreview }
  | { commandId: string; kind: "clear"; value: VmmNativeClearPreview }
  | { commandId: string; kind: "delete"; value: VmmNativeProjectDeletePreview }

// NativeProjectConfirmationRequest binds one preview to its durable mutation command identity.
// NativeProjectConfirmationRequest 将一次预检绑定到其持久变更命令身份。
export interface NativeProjectConfirmationRequest {
  // Backend action selected by the preview kind.
  // 由预检类型选择的后端动作。
  action: string
  // Stable UUID reused until this preview is either committed or cancelled.
  // 在本次预检提交或取消前持续复用的稳定 UUID。
  commandId: string
  // Exact payload frozen by the accepted preview.
  // 由已接受预检冻结的准确载荷。
  payload: Record<string, unknown>
}

// NativeProjectAccessRequest carries one atomic owner-and-recall association update.
// NativeProjectAccessRequest 携带一次原子的归属与召回关联更新。
export interface NativeProjectAccessRequest {
  // Backend action that commits the complete project access tuple in one transaction.
  // 在一个事务中提交完整项目访问元组的后端动作。
  action: "native_project_set_access"
  // Stable VulcanCode project identity, one write owner, and the ordered recall-only projects.
  // 稳定的 VulcanCode 项目标识、唯一写入归属，以及有序的仅召回项目。
  payload: {
    vulcanProjectId: string
    ownerProjectId: number
    linkedProjectIds: number[]
  }
}

// nativeProjectAccessRequest builds one indivisible owner-and-recall update for the backend.
// nativeProjectAccessRequest 为后端构造一次不可分割的归属与召回更新。
//
// Parameters: vulcanProjectId is the stable host project identity, ownerProjectId is its only
// write destination, and linkedProjectIds are ordered read-only recall extensions.
// 参数：vulcanProjectId 是稳定宿主项目身份，ownerProjectId 是唯一写入目标，
// linkedProjectIds 是有序的只读召回扩展。
//
// Returns: one `native_project_set_access` request whose list is detached from mutable UI state.
// 返回：一条 `native_project_set_access` 请求，其列表与可变界面状态相互独立。
export function nativeProjectAccessRequest(
  vulcanProjectId: string,
  ownerProjectId: number,
  linkedProjectIds: readonly number[],
): NativeProjectAccessRequest {
  return {
    action: "native_project_set_access",
    payload: {
      vulcanProjectId,
      ownerProjectId,
      linkedProjectIds: [...linkedProjectIds],
    },
  }
}

// nativeProjectConfirmationRequest builds the exact retry-safe mutation request for one preview.
// nativeProjectConfirmationRequest 为一次预检构造准确且可安全重试的变更请求。
//
// Parameters: preview is the accepted destructive preview and its stable confirmation UUID.
// 参数：preview 是已接受的破坏性预检及其稳定确认 UUID。
//
// Returns: the backend action, exact payload, and unchanged command identity.
// 返回：后端动作、准确载荷与保持不变的命令身份。
export function nativeProjectConfirmationRequest(
  preview: NativeProjectPreview,
): NativeProjectConfirmationRequest {
  if (preview.kind === "migration") {
    return {
      action: "native_project_migrate",
      commandId: preview.commandId,
      payload: {
        sourceProjectId: preview.value.sourceProjectId,
        targetProjectId: preview.value.targetProjectId,
        expectedStorageVersion: preview.value.storageVersion,
        expectedPreviewFingerprint: preview.value.previewFingerprint,
        acceptSessionRenames: preview.value.sessionRenames.length > 0,
      },
    }
  }
  if (preview.kind === "clear") {
    return {
      action: "native_project_clear",
      commandId: preview.commandId,
      payload: {
        vmmProjectId: preview.value.projectId,
        expectedStorageVersion: preview.value.storageVersion,
      },
    }
  }
  return {
    action: "native_project_delete",
    commandId: preview.commandId,
    payload: {
      vmmProjectId: preview.value.project.projectId,
      expectedStorageVersion: preview.value.storageVersion,
      confirmDetachBindings: preview.value.project.ownerVulcanProjectIds.length > 0
        || preview.value.project.linkedVulcanProjectIds.length > 0,
    },
  }
}

// VmmNativeProjectPage renders native one-level projects, stable bindings, and guarded maintenance.
// VmmNativeProjectPage 渲染原生一级项目、稳定绑定与受保护维护。
function VmmNativeProjectPage({ busy, notice, projects, onRunAction }: VmmNativeProjectPageProps) {
  const { t } = useI18n()
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [createVulcanProjectId, setCreateVulcanProjectId] = React.useState("")
  const [createProjectName, setCreateProjectName] = React.useState("")
  const [ownerDrafts, setOwnerDrafts] = React.useState<Record<string, string>>({})
  const [linkedDrafts, setLinkedDrafts] = React.useState<Record<string, number[]>>({})
  const [renameDrafts, setRenameDrafts] = React.useState<Record<number, string>>({})
  const [migrationSourceId, setMigrationSourceId] = React.useState("")
  const [migrationTargetId, setMigrationTargetId] = React.useState("")
  const [preview, setPreview] = React.useState<NativeProjectPreview | null>(null)
  const [localBusy, setLocalBusy] = React.useState(false)
  const loadRequestedRef = React.useRef(false)
  const effectiveBusy = busy || localBusy

  // acceptCatalog updates every binding draft from one authoritative native catalog response.
  // acceptCatalog 根据一次权威原生目录响应更新全部绑定草稿。
  const acceptCatalog = React.useCallback((nextCatalog: VmmNativeProjectCatalogSummary) => {
    setCatalog(nextCatalog)
    const nextOwners: Record<string, string> = {}
    const nextLinks: Record<string, number[]> = {}
    for (const binding of nextCatalog.bindings) {
      nextOwners[binding.vulcanProjectId] = String(binding.ownerProjectId)
      nextLinks[binding.vulcanProjectId] = binding.linkedProjectIds
    }
    setOwnerDrafts(nextOwners)
    setLinkedDrafts(nextLinks)
    setRenameDrafts((current) => {
      const next: Record<number, string> = {}
      for (const project of nextCatalog.projects) {
        next[project.projectId] = current[project.projectId] ?? project.name
      }
      return next
    })
  }, [])

  // runAction serializes page-owned operations and consumes every structured response kind.
  // runAction 串行化页面持有的操作并消费每种结构化响应类型。
  const runAction = React.useCallback(async (
    action: string,
    payload?: Record<string, unknown>,
    options?: MemoryActionRunOptions,
  ) => {
    if (effectiveBusy) return null
    setLocalBusy(true)
    try {
      const result = await onRunAction(action, payload, options)
      if (result?.data.kind === "native_project_catalog") acceptCatalog(result.data.value)
      if (result?.data.kind === "native_migration_preview") {
        setPreview({ commandId: crypto.randomUUID(), kind: "migration", value: result.data.value })
      }
      if (result?.data.kind === "native_clear_preview") {
        setPreview({ commandId: crypto.randomUUID(), kind: "clear", value: result.data.value })
      }
      if (result?.data.kind === "native_project_delete_preview") {
        setPreview({ commandId: crypto.randomUUID(), kind: "delete", value: result.data.value })
      }
      return result
    } finally {
      setLocalBusy(false)
    }
  }, [acceptCatalog, effectiveBusy, onRunAction])

  React.useEffect(() => {
    if (loadRequestedRef.current) return
    loadRequestedRef.current = true
    void runAction("native_project_catalog")
  }, [runAction])

  const bindingsByVulcanProject = new Map(
    (catalog?.bindings ?? []).map((binding) => [binding.vulcanProjectId, binding]),
  )
  const unboundProjects = projects.filter((project) => !bindingsByVulcanProject.has(project.projectId))

  React.useEffect(() => {
    if (unboundProjects.some((project) => project.projectId === createVulcanProjectId)) return
    const nextProjectId = unboundProjects[0]?.projectId ?? ""
    setCreateVulcanProjectId(nextProjectId)
    setCreateProjectName(nextProjectId)
  }, [createVulcanProjectId, unboundProjects])

  // saveBinding commits one owner and all recall-only links through one atomic backend action.
  // saveBinding 通过一个原子后端动作共同提交唯一归属与全部仅召回链接。
  const saveBinding = React.useCallback(async (vulcanProjectId: string) => {
    const ownerProjectId = Number(ownerDrafts[vulcanProjectId])
    const current = catalog?.bindings.find((binding) => binding.vulcanProjectId === vulcanProjectId)
    if (!ownerProjectId) {
      if (current) await runAction("native_project_unbind", { vulcanProjectId })
      return
    }
    const request = nativeProjectAccessRequest(
      vulcanProjectId,
      ownerProjectId,
      linkedDrafts[vulcanProjectId] ?? [],
    )
    await runAction(request.action, request.payload)
  }, [catalog?.bindings, linkedDrafts, ownerDrafts, runAction])

  // toggleLinkedProject changes only one recall-only project choice for one VulcanCode project.
  // toggleLinkedProject 只修改一个 VulcanCode 项目的一项仅召回项目选择。
  const toggleLinkedProject = React.useCallback((vulcanProjectId: string, projectId: number) => {
    setLinkedDrafts((current) => {
      const values = current[vulcanProjectId] ?? []
      const nextValues = values.includes(projectId)
        ? values.filter((value) => value !== projectId)
        : [...values, projectId]
      return { ...current, [vulcanProjectId]: nextValues }
    })
  }, [])

  // executePreview commits the exact preview version and never recomputes destructive scope silently.
  // executePreview 提交准确的预检版本，绝不静默重新计算破坏范围。
  const executePreview = React.useCallback(async () => {
    if (!preview) return
    // confirmation remains unchanged when a lost response leaves this preview visible for retry.
    // confirmation 在响应丢失导致本预检继续可见并重试时保持不变。
    const confirmation = nativeProjectConfirmationRequest(preview)
    const result = await runAction(
      confirmation.action,
      confirmation.payload,
      { commandId: confirmation.commandId },
    )
    if (!result) return
    setPreview(null)
    await runAction("native_project_catalog")
  }, [preview, runAction])

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("vmm.nativeProjects.title")}</h2>
          <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t("vmm.nativeProjects.description")}</p>
        </div>
        <Button disabled={effectiveBusy} onClick={() => void runAction("native_project_catalog")} type="button" variant="outline">
          <RefreshCw className="size-4" aria-hidden="true" />
          {t("vmm.nativeProjects.refresh")}
        </Button>
      </div>

      {notice ? <p className="rounded-lg border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">{notice}</p> : null}
      {effectiveBusy ? <p className="flex items-center gap-2 text-sm text-muted-foreground"><LoaderCircle className="size-4 animate-spin" aria-hidden="true" />{t("vmm.nativeProjects.running")}</p> : null}

      <Card>
        <CardHeader><CardTitle>{t("vmm.nativeProjects.createTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.createDescription")}</CardDescription></CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-[minmax(12rem,1fr)_minmax(12rem,1fr)_auto]">
          <Select onValueChange={(value) => { setCreateVulcanProjectId(value); setCreateProjectName(value) }} value={createVulcanProjectId}>
            <SelectTrigger><SelectValue placeholder={t("vmm.nativeProjects.selectVulcanProject")} /></SelectTrigger>
            <SelectContent>{unboundProjects.map((project) => <SelectItem key={project.projectId} value={project.projectId}>{project.name}</SelectItem>)}</SelectContent>
          </Select>
          <Input onChange={(event) => setCreateProjectName(event.target.value)} placeholder={t("vmm.nativeProjects.projectName")} value={createProjectName} />
          <Button disabled={effectiveBusy || !createVulcanProjectId || !createProjectName.trim()} onClick={() => void runAction("native_project_create_and_bind", { vulcanProjectId: createVulcanProjectId, projectName: createProjectName.trim() })} type="button">
            <Plus className="size-4" aria-hidden="true" />{t("vmm.nativeProjects.create")}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><Link2 className="size-4" aria-hidden="true" />{t("vmm.nativeProjects.bindingsTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.bindingsDescription")}</CardDescription></CardHeader>
        <CardContent className="space-y-3">
          {projects.map((project) => {
            const ownerId = Number(ownerDrafts[project.projectId])
            return (
              <div className="space-y-3 rounded-lg border p-3" key={project.projectId}>
                <div className="grid gap-3 md:grid-cols-[minmax(10rem,1fr)_minmax(12rem,1fr)_auto]">
                  <div className="min-w-0"><p className="truncate text-sm font-medium">{project.name}</p><p className="truncate text-xs text-muted-foreground">{project.projectId}</p></div>
                  <Select onValueChange={(value) => setOwnerDrafts((current) => ({ ...current, [project.projectId]: value === "unbound" ? "" : value }))} value={ownerDrafts[project.projectId] || "unbound"}>
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent><SelectItem value="unbound">{t("vmm.nativeProjects.unbound")}</SelectItem>{(catalog?.projects ?? []).map((item) => <SelectItem key={item.projectId} value={String(item.projectId)}>{item.name}</SelectItem>)}</SelectContent>
                  </Select>
                  <Button disabled={effectiveBusy} onClick={() => void saveBinding(project.projectId)} type="button" variant="outline"><Save className="size-4" aria-hidden="true" />{t("vmm.nativeProjects.saveBinding")}</Button>
                </div>
                {ownerId > 0 ? (
                  <div>
                    <Label>{t("vmm.nativeProjects.linkedRecall")}</Label>
                    <div className="mt-2 flex flex-wrap gap-2">
                      {(catalog?.projects ?? []).filter((item) => item.projectId !== ownerId).map((item) => (
                        <label className="flex cursor-pointer items-center gap-2 rounded-md border px-2 py-1 text-xs" key={item.projectId}>
                          <input checked={(linkedDrafts[project.projectId] ?? []).includes(item.projectId)} onChange={() => toggleLinkedProject(project.projectId, item.projectId)} type="checkbox" />
                          {item.name}
                        </label>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            )
          })}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>{t("vmm.nativeProjects.catalogTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.catalogDescription")}</CardDescription></CardHeader>
        <CardContent className="space-y-3">
          {(catalog?.projects ?? []).map((project) => (
            <div className="grid gap-3 rounded-lg border p-3 lg:grid-cols-[minmax(12rem,1fr)_auto_auto_auto]" key={project.projectId}>
              <div className="space-y-1">
                <div className="flex items-center gap-2"><Input onChange={(event) => setRenameDrafts((current) => ({ ...current, [project.projectId]: event.target.value }))} value={renameDrafts[project.projectId] ?? project.name} /><Button disabled={effectiveBusy || !(renameDrafts[project.projectId] ?? "").trim()} onClick={() => void runAction("native_project_rename", { vmmProjectId: project.projectId, name: renameDrafts[project.projectId]?.trim() })} size="icon" title={t("vmm.nativeProjects.rename")} type="button" variant="outline"><Pencil className="size-4" aria-hidden="true" /></Button></div>
                <p className="text-xs text-muted-foreground">{project.tag}</p>
              </div>
              <Button disabled={effectiveBusy} onClick={() => void runAction("native_project_clear_preview", { vmmProjectId: project.projectId })} type="button" variant="outline"><DatabaseZap className="size-4" aria-hidden="true" />{t("vmm.nativeProjects.clear")}</Button>
              <Button disabled={effectiveBusy || project.protected} onClick={() => void runAction("native_project_delete_preview", { vmmProjectId: project.projectId })} type="button" variant="destructive"><Trash2 className="size-4" aria-hidden="true" />{project.protected ? t("vmm.nativeProjects.protected") : t("vmm.nativeProjects.delete")}</Button>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>{t("vmm.nativeProjects.migrateTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.migrateDescription")}</CardDescription></CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-[1fr_1fr_auto]">
          <NativeProjectSelect catalog={catalog} onChange={setMigrationSourceId} placeholder={t("vmm.nativeProjects.source")} value={migrationSourceId} />
          <NativeProjectSelect catalog={catalog} onChange={setMigrationTargetId} placeholder={t("vmm.nativeProjects.target")} value={migrationTargetId} />
          <Button disabled={effectiveBusy || !migrationSourceId || !migrationTargetId || migrationSourceId === migrationTargetId} onClick={() => void runAction("native_project_migration_preview", { sourceProjectId: Number(migrationSourceId), targetProjectId: Number(migrationTargetId) })} type="button" variant="outline">{t("vmm.nativeProjects.previewMigration")}</Button>
        </CardContent>
      </Card>

      {preview ? <NativePreviewCard busy={effectiveBusy} onCancel={() => setPreview(null)} onConfirm={() => void executePreview()} preview={preview} /> : null}
    </div>
  )
}

// NativeProjectSelect renders one native project selector with an explicit placeholder.
// NativeProjectSelect 渲染一个带明确占位文案的原生项目选择器。
function NativeProjectSelect({ catalog, onChange, placeholder, value }: {
  catalog: VmmNativeProjectCatalogSummary | null
  onChange: (value: string) => void
  placeholder: string
  value: string
}) {
  return <Select onValueChange={onChange} value={value}><SelectTrigger><SelectValue placeholder={placeholder} /></SelectTrigger><SelectContent>{(catalog?.projects ?? []).map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name}</SelectItem>)}</SelectContent></Select>
}

// NativePreviewCard renders exact counts and the only commit action for a destructive preview.
// NativePreviewCard 渲染准确计数与破坏性预检的唯一提交动作。
function NativePreviewCard({ busy, onCancel, onConfirm, preview }: {
  busy: boolean
  onCancel: () => void
  onConfirm: () => void
  preview: NativeProjectPreview
}) {
  const { t } = useI18n()
  const content = preview.kind === "migration" ? preview.value.source : preview.kind === "clear" ? preview.value.removed : preview.value.content
  const owners = preview.kind === "delete" ? preview.value.project.ownerVulcanProjectIds : []
  const links = preview.kind === "delete" ? preview.value.project.linkedVulcanProjectIds : []
  const conflicts = preview.kind === "migration" ? preview.value.sessionRenames.length : 0
  const titleKey: TranslationKey = preview.kind === "migration"
    ? "vmm.nativeProjects.preview.migration"
    : preview.kind === "clear"
      ? "vmm.nativeProjects.preview.clear"
      : "vmm.nativeProjects.preview.delete"
  return (
    <Alert variant={preview.kind === "delete" ? "destructive" : "default"}>
      {conflicts > 0 || preview.kind === "delete" ? <CircleAlert aria-hidden="true" /> : <DatabaseZap aria-hidden="true" />}
      <AlertTitle>{t(titleKey)}</AlertTitle>
      <AlertDescription className="space-y-3">
        {preview.kind === "migration" ? (
          <p>
            {t("vmm.nativeProjects.previewMigrationCounts")
              .replace("{sourceSessions}", String(preview.value.source.sessions))
              .replace("{sourceMemories}", String(preview.value.source.memories))
              .replace("{sourceProfiles}", String(preview.value.source.profileNodes))
              .replace("{sourceVectors}", String(preview.value.source.vectorRows))
              .replace("{targetSessions}", String(preview.value.targetBefore.sessions))
              .replace("{targetMemories}", String(preview.value.targetBefore.memories))
              .replace("{targetProfiles}", String(preview.value.targetBefore.profileNodes))
              .replace("{targetVectors}", String(preview.value.targetBefore.vectorRows))
              .replace("{afterSessions}", String(preview.value.targetAfter.sessions))
              .replace("{afterMemories}", String(preview.value.targetAfter.memories))
              .replace("{afterProfiles}", String(preview.value.targetAfter.profileNodes))
              .replace("{afterVectors}", String(preview.value.targetAfter.vectorRows))}
          </p>
        ) : (
          <p>{t("vmm.nativeProjects.previewCounts").replace("{sessions}", String(content.sessions)).replace("{memories}", String(content.memories)).replace("{profiles}", String(content.profileNodes)).replace("{vectors}", String(content.vectorRows))}</p>
        )}
        {preview.kind === "migration" ? <p>{t("vmm.nativeProjects.previewConflicts").replace("{count}", String(conflicts))}</p> : null}
        {preview.kind === "migration" ? (
          <details className="rounded-md border bg-background/60 p-2">
            <summary className="cursor-pointer text-sm font-medium">
              {t("vmm.nativeProjects.previewRecords").replace("{count}", String(preview.value.records.length))}
            </summary>
            <div className="mt-2 max-h-72 space-y-2 overflow-auto">
              {preview.value.records.map((record) => (
                <div className="rounded border px-2 py-1.5 text-xs" key={`${record.recordKind}:${record.recordId}`}>
                  <p className="font-medium">
                    {t(`vmm.nativeProjects.recordKind.${record.recordKind}` as TranslationKey)} · {record.recordId}
                  </p>
                  {record.sessionSourceId && record.sessionSourceId !== record.recordId ? (
                    <p className="text-muted-foreground">{t("vmm.nativeProjects.recordSession").replace("{session}", record.sessionSourceId)}</p>
                  ) : null}
                  {record.summary ? <p className="mt-1 whitespace-pre-wrap text-muted-foreground">{record.summary}</p> : null}
                </div>
              ))}
            </div>
          </details>
        ) : null}
        {owners.length > 0 ? <p>{t("vmm.nativeProjects.previewOwners").replace("{projects}", owners.join(", "))}</p> : null}
        {links.length > 0 ? <p>{t("vmm.nativeProjects.previewLinks").replace("{projects}", links.join(", "))}</p> : null}
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={onConfirm} type="button" variant={preview.kind === "delete" ? "destructive" : "default"}>
            {conflicts > 0 ? <CircleAlert className="size-4" aria-hidden="true" /> : null}
            {conflicts > 0 ? t("vmm.nativeProjects.confirmRenameMigration") : t("vmm.nativeProjects.confirm")}
          </Button>
          <Button disabled={busy} onClick={onCancel} type="button" variant="outline">{t("vmm.setup.cancel")}</Button>
        </div>
      </AlertDescription>
    </Alert>
  )
}

export { VmmNativeProjectPage }
export type { VmmNativeProjectPageProps }
