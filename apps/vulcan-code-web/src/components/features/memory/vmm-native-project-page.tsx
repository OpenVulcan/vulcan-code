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

import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { Button } from "@/ui-kit/primitives/button"
import { PageTabs } from "@/components/workspace/workspace-directory-layout"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/ui-kit/primitives/dialog"
import { Input } from "@/ui-kit/primitives/input"
import { Label } from "@/ui-kit/primitives/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
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
import { VmmNativeProjectRetention } from "./vmm-native-project-retention"
import { VmmNativeProjectProfilePolicy } from "./vmm-native-project-profile-policy"
import { VmmProjectDirectory } from "./vmm-project-directory"
import "@/ui-kit/styles/features/settings-forms.css"
import "@/ui-kit/styles/features/memory.css"

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
  // projectView separates binding, storage, and lifecycle management.
  // projectView 分离绑定、存储与生命周期管理。
  const [projectView, setProjectView] = React.useState("bindings")
  // memoryProjectId is the shared explicit target for storage management and both rule groups.
  // memoryProjectId 是存储管理及两个规则组共享的显式目标。
  const [memoryProjectId, setMemoryProjectId] = React.useState("")
  // projectRuleGroup serializes initial rule reads through the shared action channel.
  // projectRuleGroup 通过共享动作通道串行化规则首次读取。
  const [projectRuleGroup, setProjectRuleGroup] = React.useState("retention")
  // selectedHostId initially opens the first catalog-backed work project.
  // selectedHostId 初始打开目录中的首个工作项目。
  const [selectedHostId, setSelectedHostId] = React.useState(projects[0]?.projectId ?? "")
  // Linked-project search is scoped to the current editor and never removes hidden selections.
  // 关联项目搜索只作用于当前编辑器，绝不删除隐藏的选择。
  const [linkedSearch, setLinkedSearch] = React.useState("")
  // createOpen moves occasional creation out of the primary binding workspace.
  // createOpen 将低频创建移出主要绑定工作区。
  const [createOpen, setCreateOpen] = React.useState(false)
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [createVulcanProjectId, setCreateVulcanProjectId] = React.useState("")
  const [createProjectName, setCreateProjectName] = React.useState("")
  const [ownerDrafts, setOwnerDrafts] = React.useState<Record<string, string>>({})
  const [linkedDrafts, setLinkedDrafts] = React.useState<Record<string, number[]>>({})
  // Accepted binding values distinguish local edits from unchanged fields during catalog refresh.
  // 已接受绑定值在目录刷新时区分本地编辑与未修改字段。
  const acceptedBindingsRef = React.useRef<{ owners: Record<string, string>; links: Record<string, number[]> }>({ owners: {}, links: {} })
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
    // Keep an existing stable selection; otherwise select the first available catalog record.
    // 保留已有稳定选择；否则选择目录中首个可用记录。
    setMemoryProjectId((current) => nextCatalog.projects.some((project) => String(project.projectId) === current) ? current : nextCatalog.projects.length ? String(nextCatalog.projects[0].projectId) : "")
    const nextOwners: Record<string, string> = {}
    const nextLinks: Record<string, number[]> = {}
    for (const binding of nextCatalog.bindings) {
      nextOwners[binding.vulcanProjectId] = String(binding.ownerProjectId)
      nextLinks[binding.vulcanProjectId] = binding.linkedProjectIds
    }
    // Preserve dirty fields across refresh and project switching; accepted server changes update clean fields only.
    // 跨刷新及项目切换保留脏字段；已接受的服务端变化仅更新未编辑字段。
    const previous = acceptedBindingsRef.current
    acceptedBindingsRef.current = { owners: nextOwners, links: nextLinks }
    setOwnerDrafts((current) => ({ ...nextOwners, ...Object.fromEntries(Object.entries(current).filter(([id, value]) => value !== (previous.owners[id] ?? ""))) }))
    setLinkedDrafts((current) => ({ ...nextLinks, ...Object.fromEntries(Object.entries(current).filter(([id, values]) => JSON.stringify(values) !== JSON.stringify(previous.links[id] ?? []))) }))
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
    if (loadRequestedRef.current || effectiveBusy) return
    loadRequestedRef.current = true
    void runAction("native_project_catalog")
  }, [effectiveBusy, runAction])

  const bindingsByVulcanProject = new Map(
    (catalog?.bindings ?? []).map((binding) => [binding.vulcanProjectId, binding]),
  )
  const unboundProjects = projects.filter((project) => !bindingsByVulcanProject.has(project.projectId))
  // Keep removed host projects from leaving an unreachable detail selection.
  // 避免已移除的宿主项目留下不可达详情选择。
  React.useEffect(() => {
    if (!projects.some((project) => project.projectId === selectedHostId)) setSelectedHostId(projects[0]?.projectId ?? "")
  }, [projects, selectedHostId])

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
    <fieldset disabled={effectiveBusy} className="vk-form-loose">
      <PageTabs
        actions={<>
          <Button disabled={effectiveBusy} onClick={() => void runAction("native_project_catalog")} size="sm" type="button" variant="outline">
            <RefreshCw className="vk-icon" aria-hidden="true" />
            {t("vmm.nativeProjects.refresh")}
          </Button>
          <Button disabled={effectiveBusy} onClick={() => setCreateOpen(true)} size="sm" type="button"><Plus className="vk-icon" aria-hidden="true" />{t("vmm.nativeProjects.createTitle")}</Button>
        </>}
        ariaLabel={t("memory.identity.nativeTitle")}
        className="vmm-page-tabs"
        onValueChange={setProjectView}
        options={([["bindings", "vmm.nativeProjects.bindingsTitle"], ["catalog", "vmm.nativeProjects.catalogTitle"], ["rules", "memory.workspace.projectRules"], ["migration", "vmm.nativeProjects.migrateTitle"]] as const).map(([value, label]) => ({ label: t(label), value }))}
        value={projectView}
      />

      {notice ? <p className="vk-form-well">{notice}</p> : null}
      {effectiveBusy ? <p className="vk-form-status"><LoaderCircle className="vk-spin" aria-hidden="true" />{t("vmm.nativeProjects.running")}</p> : null}

      <Dialog open={createOpen} onOpenChange={(open) => { if (!effectiveBusy) setCreateOpen(open) }}><DialogContent>
        <DialogHeader><DialogTitle>{t("vmm.nativeProjects.createTitle")}</DialogTitle><DialogDescription>{t("vmm.nativeProjects.createDescription")}</DialogDescription></DialogHeader>
        <div className="vk-form-stack">
          <Select disabled={effectiveBusy} onValueChange={(value) => { setCreateVulcanProjectId(value); setCreateProjectName(value) }} value={createVulcanProjectId}>
            <SelectTrigger><SelectValue placeholder={t("vmm.nativeProjects.selectVulcanProject")} /></SelectTrigger>
            <SelectContent>{unboundProjects.map((project) => <SelectItem key={project.projectId} value={project.projectId}>{project.name}</SelectItem>)}</SelectContent>
          </Select>
          <Input disabled={effectiveBusy} onChange={(event) => setCreateProjectName(event.target.value)} placeholder={t("vmm.nativeProjects.projectName")} value={createProjectName} />
          <Button disabled={effectiveBusy || !createVulcanProjectId || !createProjectName.trim()} onClick={async () => { const response = await runAction("native_project_create_and_bind", { vulcanProjectId: createVulcanProjectId, projectName: createProjectName.trim() }); if (response?.data.kind === "native_project_catalog") setCreateOpen(false) }} type="button">
            <Plus className="vk-icon" aria-hidden="true" />{t("vmm.nativeProjects.create")}
          </Button>
        </div>
      </DialogContent></Dialog>

      <div hidden={projectView !== "bindings"}><div className="vmm-project-workspace">
      <VmmProjectDirectory entries={projects.map((project) => ({ value: project.projectId, label: project.name, description: project.path, bound: bindingsByVulcanProject.has(project.projectId), hasOverride: project.hasOverride }))} value={selectedHostId} disabled={effectiveBusy} onChange={setSelectedHostId} />
      <Card>
        <CardHeader><CardTitle className="vk-form-inline"><Link2 className="vk-icon" aria-hidden="true" />{t("vmm.nativeProjects.bindingsTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.bindingsDescription")}</CardDescription></CardHeader>
        <CardContent className="vk-form-stack">
          {projects.filter((project) => project.projectId === selectedHostId).map((project) => {
            const ownerId = Number(ownerDrafts[project.projectId])
            // Compare this editor with the accepted binding; project navigation never discards its draft.
            // 将本编辑器与已接受绑定比较；项目导航绝不丢弃其草稿。
            const savedBinding = bindingsByVulcanProject.get(project.projectId)
            const bindingDirty = (ownerDrafts[project.projectId] ?? "") !== (savedBinding ? String(savedBinding.ownerProjectId) : "") || JSON.stringify(linkedDrafts[project.projectId] ?? []) !== JSON.stringify(savedBinding?.linkedProjectIds ?? [])
            return (
              <div className="vk-form-stack" key={project.projectId}>
                <div className="vk-mem-link-grid">
                  <div className="vk-form-cell"><p className="vk-form-name">{project.name}</p><p className="vk-form-note vk-truncate">{project.projectId}</p></div>
                  <Select disabled={effectiveBusy} onValueChange={(value) => { setOwnerDrafts((current) => ({ ...current, [project.projectId]: value === "unbound" ? "" : value })); setLinkedDrafts((current) => ({ ...current, [project.projectId]: value === "unbound" ? [] : (current[project.projectId] ?? []).filter((id) => id !== Number(value)) })) }} value={ownerDrafts[project.projectId] || "unbound"}>
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent><SelectItem value="unbound">{t("vmm.nativeProjects.unbound")}</SelectItem>{(catalog?.projects ?? []).map((item) => <SelectItem key={item.projectId} value={String(item.projectId)}>{item.name}</SelectItem>)}</SelectContent>
                  </Select>
                  <Button disabled={effectiveBusy || !bindingDirty} onClick={() => void saveBinding(project.projectId)} type="button" variant="outline"><Save className="vk-icon" aria-hidden="true" />{t("vmm.nativeProjects.saveBinding")}</Button>
                </div>
                {bindingDirty ? <p className="vk-form-note">{t("composer.capability.saveHint.unsaved")}</p> : null}
                {ownerId > 0 ? (
                  <div>
                    <Label>{t("vmm.nativeProjects.linkedRecall")}</Label>
                    <p className="vk-form-note vk-mem-gap">{t("memory.workspace.selectedCount").replace("{count}", String((linkedDrafts[project.projectId] ?? []).filter((id) => id !== ownerId).length))}</p>
                    <Input aria-label={t("memory.profile.searchProject")} placeholder={t("memory.profile.searchProject")} value={linkedSearch} onChange={(event) => setLinkedSearch(event.target.value)} />
                    <div className="vmm-linked-project-list">
                      {(catalog?.projects ?? []).filter((item) => item.projectId !== ownerId && `${item.name} ${item.tag}`.toLocaleLowerCase().includes(linkedSearch.trim().toLocaleLowerCase())).map((item) => (
                        <label className="vk-check-label vk-mem-check" key={item.projectId}>
                          <input disabled={effectiveBusy} checked={(linkedDrafts[project.projectId] ?? []).includes(item.projectId)} onChange={() => toggleLinkedProject(project.projectId, item.projectId)} type="checkbox" />
                          <span className="vk-form-cell"><span className="vk-truncate vk-mem-line" title={item.name}>{item.name}</span><span className="vk-form-note">{item.tag}</span></span>
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

      </div></div>
      <div hidden={projectView !== "catalog"}><div className="vmm-project-workspace">
      <VmmProjectDirectory entries={(catalog?.projects ?? []).map((project) => ({ value: String(project.projectId), label: project.name, description: project.tag, bound: (catalog?.bindings ?? []).some((binding) => binding.ownerProjectId === project.projectId), hasOverride: false }))} value={memoryProjectId} disabled={effectiveBusy} allowOverrides={false} onChange={setMemoryProjectId} />
      <Card>
        <CardHeader><CardTitle>{t("vmm.nativeProjects.catalogTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.catalogDescription")}</CardDescription></CardHeader>
        <CardContent className="vk-form-stack">
          {(catalog?.projects ?? []).filter((project) => String(project.projectId) === memoryProjectId).map((project) => (
            <div className="vk-form-box vk-mem-project-row" key={project.projectId}>
              <div className="vk-form-tight">
                <div className="vk-form-inline"><Input onChange={(event) => setRenameDrafts((current) => ({ ...current, [project.projectId]: event.target.value }))} value={renameDrafts[project.projectId] ?? project.name} /><Button disabled={effectiveBusy || !(renameDrafts[project.projectId] ?? "").trim()} onClick={() => void runAction("native_project_rename", { vmmProjectId: project.projectId, name: renameDrafts[project.projectId]?.trim() })} size="icon" title={t("vmm.nativeProjects.rename")} type="button" variant="outline"><Pencil className="vk-icon" aria-hidden="true" /></Button></div>
                <p className="vk-form-note">{project.tag}</p>
              </div>
              <Button disabled={effectiveBusy} onClick={() => void runAction("native_project_clear_preview", { vmmProjectId: project.projectId })} type="button" variant="outline"><DatabaseZap className="vk-icon" aria-hidden="true" />{t("vmm.nativeProjects.clear")}</Button>
              <Button disabled={effectiveBusy || project.protected} onClick={() => void runAction("native_project_delete_preview", { vmmProjectId: project.projectId })} type="button" variant="destructive"><Trash2 className="vk-icon" aria-hidden="true" />{project.protected ? t("vmm.nativeProjects.protected") : t("vmm.nativeProjects.delete")}</Button>
            </div>
          ))}
        </CardContent>
      </Card>

      </div></div>
      <div hidden={projectView !== "migration"}><Card>
        <CardHeader><CardTitle>{t("vmm.nativeProjects.migrateTitle")}</CardTitle><CardDescription>{t("vmm.nativeProjects.migrateDescription")}</CardDescription></CardHeader>
        <CardContent className="vk-form-cols3">
          <NativeProjectSelect catalog={catalog} onChange={setMigrationSourceId} placeholder={t("vmm.nativeProjects.source")} value={migrationSourceId} />
          <NativeProjectSelect catalog={catalog} onChange={setMigrationTargetId} placeholder={t("vmm.nativeProjects.target")} value={migrationTargetId} />
          <Button disabled={effectiveBusy || !migrationSourceId || !migrationTargetId || migrationSourceId === migrationTargetId} onClick={() => void runAction("native_project_migration_preview", { sourceProjectId: Number(migrationSourceId), targetProjectId: Number(migrationTargetId) })} type="button" variant="outline">{t("vmm.nativeProjects.previewMigration")}</Button>
        </CardContent>
      </Card>

      </div>
      <div hidden={projectView !== "rules"} className="vk-form-loose">
      {/* The edited project sits in the rule-group tab row, so both rule groups visibly share one target.
          被编辑项目位于规则分组标签行内，使两个规则分组明确共用同一目标。 */}
      <PageTabs
        actions={<div className="vmm-inline-field">
          <Label>{t("memory.projectRetention.select")}</Label>
          <NativeProjectSelect ariaLabel={t("memory.projectRetention.select")} catalog={catalog} onChange={setMemoryProjectId} placeholder={t("memory.projectRetention.select")} value={memoryProjectId} />
        </div>}
        ariaLabel={t("memory.workspace.projectRules")}
        onValueChange={setProjectRuleGroup}
        options={[{ label: t("memory.projectRetention.title"), value: "retention" }, { label: t("memory.projectProfile.title"), value: "profile" }]}
        value={projectRuleGroup}
      />
      {catalog && memoryProjectId ? <>
        <div hidden={projectRuleGroup !== "retention"}><VmmNativeProjectRetention selectedProjectId={Number(memoryProjectId)} projects={catalog.projects} busy={effectiveBusy || projectView !== "rules" || projectRuleGroup !== "retention"} onRunAction={runAction} /></div>
        <div hidden={projectRuleGroup !== "profile"}><VmmNativeProjectProfilePolicy selectedProjectId={Number(memoryProjectId)} projects={catalog.projects} busy={effectiveBusy || projectView !== "rules" || projectRuleGroup !== "profile"} onRunAction={runAction} /></div>
      </> : null}
      </div>
      <Dialog open={preview !== null} onOpenChange={(open) => { if (!open && !effectiveBusy) setPreview(null) }}><DialogContent size="lg"><DialogHeader><DialogTitle>{t(preview?.kind === "delete" ? "vmm.nativeProjects.preview.delete" : preview?.kind === "clear" ? "vmm.nativeProjects.preview.clear" : "vmm.nativeProjects.preview.migration")}</DialogTitle><DialogDescription>{t("vmm.nativeProjects.description")}</DialogDescription></DialogHeader>
      {preview ? <NativePreviewCard busy={effectiveBusy} onCancel={() => setPreview(null)} onConfirm={() => void executePreview()} preview={preview} /> : null}
      </DialogContent></Dialog>
    </fieldset>
  )
}

// NativeProjectSelect renders one native project selector with an explicit placeholder.
// NativeProjectSelect 渲染一个带明确占位文案的原生项目选择器。
function NativeProjectSelect({ ariaLabel, catalog, onChange, placeholder, value }: {
  ariaLabel?: string
  catalog: VmmNativeProjectCatalogSummary | null
  onChange: (value: string) => void
  placeholder: string
  value: string
}) {
  return <Select onValueChange={onChange} value={value}><SelectTrigger aria-label={ariaLabel}><SelectValue placeholder={placeholder} /></SelectTrigger><SelectContent>{(catalog?.projects ?? []).map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name}</SelectItem>)}</SelectContent></Select>
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
      <AlertDescription className="vk-form-stack">
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
          <details className="vk-mem-disclosure">
            <summary className="vk-form-strong">
              {t("vmm.nativeProjects.previewRecords").replace("{count}", String(preview.value.records.length))}
            </summary>
            <div className="vk-form-stack vk-mem-scroll-list">
              {preview.value.records.map((record) => (
                <div className="vk-mem-record" key={`${record.recordKind}:${record.recordId}`}>
                  <p className="vk-form-em">
                    {t(`vmm.nativeProjects.recordKind.${record.recordKind}` as TranslationKey)} · {record.recordId}
                  </p>
                  {record.sessionSourceId && record.sessionSourceId !== record.recordId ? (
                    <p className="vk-faint">{t("vmm.nativeProjects.recordSession").replace("{session}", record.sessionSourceId)}</p>
                  ) : null}
                  {record.summary ? <p className="vk-form-prose vk-faint">{record.summary}</p> : null}
                </div>
              ))}
            </div>
          </details>
        ) : null}
        {owners.length > 0 ? <p>{t("vmm.nativeProjects.previewOwners").replace("{projects}", owners.join(", "))}</p> : null}
        {links.length > 0 ? <p>{t("vmm.nativeProjects.previewLinks").replace("{projects}", links.join(", "))}</p> : null}
        <div className="vk-form-chips">
          <Button disabled={busy} onClick={onConfirm} type="button" variant={preview.kind === "delete" ? "destructive" : "default"}>
            {conflicts > 0 ? <CircleAlert className="vk-icon" aria-hidden="true" /> : null}
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
