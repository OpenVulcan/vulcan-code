import * as React from "react"
import { AlertTriangle, ArchiveRestore, Clock, DatabaseZap, Fingerprint, RefreshCw, ShieldCheck, Sparkles, Trash2 } from "lucide-react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { Progress } from "@/ui-kit/primitives/progress"
import { Button } from "@/ui-kit/primitives/button"
import { PageTabs } from "@/components/workspace/workspace-directory-layout"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/ui-kit/primitives/card"
import { Label } from "@/ui-kit/primitives/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/ui-kit/primitives/tooltip"
import { useI18n } from "@/i18n"
import { runVmmMemoryAction } from "@/service/memory-actions-service"
import { VmmNativeCleanupPreview } from "./vmm-native-cleanup-preview"
import { VmmNativeLexicalIndex } from "./vmm-native-lexical-index"
import { VmmNativeRecycleRecords } from "./vmm-native-recycle-records"
import { formatSkillText } from "@/components/features/featureText"
import { formatNativeDuration } from "./native-duration-display"
import { nativeVectorRouteCause } from "./vmm-native-vector-status"
import type { MemoryActionRunOptions } from "./use-memory-area-controller"
import type {
  VmmMemoryActionResult,
  VmmNativeProjectCatalogSummary,
  VmmNativeRecycleBatchSummary,
  VmmNativeRecycleBinSummary,
  VmmNativeRetentionStatusSummary,
  VmmNativeRetentionPreview,
  VmmNativeLexicalIndexStatus,
  VmmNativeVectorAdoptionResult,
  VmmNativeVectorRebuildPreview,
  VmmNativeVectorRebuildProgress,
  VmmNativeVectorStatusSummary,
} from "@/types"
import { formatDateTime } from "@/lib/display-format"
import "@/ui-kit/styles/features/settings-forms.css";
import "@/ui-kit/styles/features/memory.css"

// VmmNativeMaintenancePageProps binds native retention controls to the shared action channel.
// VmmNativeMaintenancePageProps 将原生保留控制绑定到共享动作通道。
interface VmmNativeMaintenancePageProps {
  busy: boolean
  // A repair link may open the index tab directly without executing a rebuild.
  // 修复入口可以直达索引页，但不会直接执行重建。
  initialView?: "recycle" | "index"
  onRunAction: (
    action: string,
    payload?: Record<string, unknown>,
    options?: MemoryActionRunOptions,
  ) => Promise<VmmMemoryActionResult | null>
}

// RecycleConfirmation identifies the selected irreversible or restorative recycle operation.
// RecycleConfirmation 标识所选的不可逆或恢复型回收操作。
interface RecycleConfirmation {
  action: "restore" | "purge"
  batch: VmmNativeRecycleBatchSummary
  // storageVersion freezes the observed batch snapshot, not a later refresh.
  // storageVersion 冻结观察到的批次快照，不使用后续刷新版本。
  storageVersion: number
}

// VectorRebuildProgress shows committed counts against the confirmed preview and any active quota wait.
// VectorRebuildProgress 按确认的预检结果展示已提交数量及当前限流等待时间。
function VectorRebuildProgress({ preview, progress, running }: {
  // The confirmed preview supplies totals for both progress counts.
  // 已确认的预检结果提供两种进度计数的总数。
  preview: VmmNativeVectorRebuildPreview
  // The service snapshot supplies completed counts and remaining wait seconds.
  // 服务快照提供已完成的计数和剩余等待秒数。
  progress: VmmNativeVectorRebuildProgress
  // Show the countdown only while the rebuild is running.
  // 仅在重建运行期间展示倒计时。
  running: boolean
}) {
  const { t } = useI18n()
  return (
    <div className="vk-form-tight">
      <Progress aria-label={t("memory.maintenance.rebuildVectors")} max={Math.max(1, preview.rebuildMemoryCount)} value={progress.rebuiltMemoryCount} />
      <p className="vk-form-note">{formatSkillText(t("memory.maintenance.vectorRebuildProgress"), {
        memories: progress.rebuiltMemoryCount, totalMemories: preview.rebuildMemoryCount,
        batches: progress.batchCount, totalBatches: preview.batchCount,
      })}</p>
      {running && progress.retryAfterSeconds ? <p role="status" className="vk-form-note">{formatSkillText(t("memory.maintenance.vectorRebuildQuotaWait"), {
        seconds: progress.retryAfterSeconds,
      })}</p> : null}
    </div>
  )
}

// VmmNativeMaintenancePage renders scheduled cleanup status and the native recycle bin.
// VmmNativeMaintenancePage 渲染定期清洗状态与原生回收站。
export function VmmNativeMaintenancePage({ busy, initialView = "recycle", onRunAction }: VmmNativeMaintenancePageProps) {
  const { t, locale } = useI18n()
  // maintenanceView makes recovery the first task while retaining all maintenance controls.
  // maintenanceView 将恢复作为首要任务，同时保留全部维护控件。
  const [maintenanceView, setMaintenanceView] = React.useState<"recycle" | "cleanup" | "index">(initialView)
  const [status, setStatus] = React.useState<VmmNativeRetentionStatusSummary | null>(null)
  // Full-text state is independent of the embedding route and vector generation.
  // 全文状态独立于嵌入路由与向量代次。
  const [lexicalStatus, setLexicalStatus] = React.useState<VmmNativeLexicalIndexStatus | null>(null)
  const [vectorStatus, setVectorStatus] = React.useState<VmmNativeVectorStatusSummary | null>(null)
  const [vectorPreview, setVectorPreview] = React.useState<VmmNativeVectorRebuildPreview | null>(null)
  // The confirmed command ID correlates status reads with the running rebuild.
  // 已确认命令标识将状态读取关联到正在执行的重建。
  const [activeRebuildId, setActiveRebuildId] = React.useState<string | null>(null)
  const [rebuildProgress, setRebuildProgress] = React.useState<VmmNativeVectorRebuildProgress | null>(null)
  // The latest keep-vectors outcome is shown in the dialog when refused and on the index tab when done.
  // 最近一次沿用向量的结果：被拒绝时显示在弹窗内，完成时显示在索引页。
  const [vectorAdoption, setVectorAdoption] = React.useState<VmmNativeVectorAdoptionResult | null>(null)
  const [catalog, setCatalog] = React.useState<VmmNativeProjectCatalogSummary | null>(null)
  const [recycleBin, setRecycleBin] = React.useState<VmmNativeRecycleBinSummary | null>(null)
  const [projectFilter, setProjectFilter] = React.useState("all")
  // The current filter is read by full refreshes without restarting all statuses on selection.
  // 完整刷新读取当前筛选，选择项目时不重新读取全部状态。
  const projectFilterRef = React.useRef("all")
  const [confirmation, setConfirmation] = React.useState<RecycleConfirmation | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // Keep confirmation independent of the recycle filter and guard local cleanup submissions.
  // 确认独立于回收站筛选，并保护本地清洗提交。
  const [cleanupPreview, setCleanupPreview] = React.useState<VmmNativeRetentionPreview | null>(null)
  const [cleanupPending, setCleanupPending] = React.useState(false)
  // cleanupTrigger restores keyboard position after the asynchronously prepared preview closes.
  // cleanupTrigger 在异步准备的预览关闭后恢复键盘位置。
  const cleanupTrigger = React.useRef<HTMLButtonElement>(null)
  // Confirmation origins retain keyboard position after asynchronously prepared dialogs close.
  // 确认来源在异步准备的弹窗关闭后保留键盘位置。
  const vectorTrigger = React.useRef<HTMLButtonElement>(null)
  const recycleTrigger = React.useRef<HTMLButtonElement | null>(null)
  // Page generations stop obsolete sequential reads from consuming the shared action channel.
  // 页面代次阻止过时串行读取继续占用共享动作通道。
  const pageGeneration = React.useRef(0)
  const mounted = React.useRef(false)
  const [pagePending, setPagePending] = React.useState(true)
  // A local lock also protects confirmations before the parent's busy prop is rendered.
  // 本地锁同时保护父级忙碌属性渲染之前的确认操作。
  const actionLock = React.useRef(false)
  const [actionPending, setActionPending] = React.useState(false)
  const controlsBusy = busy || cleanupPending || pagePending || actionPending

  React.useEffect(() => {
    if (!activeRebuildId) return
    let active = true
    let timer: ReturnType<typeof setTimeout>
    // Read outside the exclusive action channel so the rebuild can continue while it is observed.
    // 在独占动作通道之外读取进度，让重建在被观察时继续执行。
    const poll = async () => {
      try {
        const { data } = await runVmmMemoryAction({
          action: "native_vector_rebuild_progress", commandId: activeRebuildId, scope: "global",
        })
        if (active && data.kind === "native_vector_rebuild_progress" && data.value) setRebuildProgress(data.value)
      } catch {
        // The rebuild response owns operation errors; a failed status read can be retried.
        // 重建响应负责操作错误，状态读取失败可以重试。
      }
      if (active) timer = setTimeout(() => void poll(), 1000)
    }
    timer = setTimeout(() => void poll(), 500)
    return () => { active = false; clearTimeout(timer) }
  }, [activeRebuildId])

  // loadStatus refreshes the process-local schedule and latest execution outcome.
  // loadStatus 刷新进程内调度状态与最近执行结果。
  const loadStatus = React.useCallback(async (generation = pageGeneration.current) => {
    const result = await onRunAction("native_retention_status")
    if (!mounted.current || generation !== pageGeneration.current) return false
    if (result?.data.kind !== "native_retention_status") {
      setStatus(null)
      setError(t("memory.maintenance.invalidResponse"))
      return false
    }
    setStatus(result.data.value)
    return true
  }, [onRunAction, t])

  // loadVectorStatus refreshes exact compatibility with the configured embedding generation.
  // loadVectorStatus 刷新与已配置 Embedding 代次的准确兼容状态。
  const loadVectorStatus = React.useCallback(async (generation = pageGeneration.current) => {
    try {
      // Preserve the transport failure separately from a completed response with the wrong kind.
      // 将传输失败与已完成但类型错误的响应分别保留。
      const result = await onRunAction("native_vector_status", undefined, { propagateError: true })
      if (!mounted.current || generation !== pageGeneration.current || !result) return false
      if (result.data.kind !== "native_vector_status") {
        setVectorStatus(null)
        setError(t("memory.maintenance.invalidResponse"))
        return false
      }
      setVectorStatus(result.data.value)
      return true
    } catch (cause) {
      if (!mounted.current || generation !== pageGeneration.current) return false
      setVectorStatus(null)
      setError(cause instanceof Error ? cause.message : String(cause))
      return false
    }
  }, [onRunAction, t])

  // loadLexicalStatus reads versioned source/index counts without calling an AI model.
  // loadLexicalStatus 读取带版本的源及索引计数，不调用 AI 模型。
  const loadLexicalStatus = React.useCallback(async (generation = pageGeneration.current) => {
    const result = await onRunAction("native_lexical_status")
    if (!mounted.current || generation !== pageGeneration.current) return false
    if (result?.data.kind !== "native_lexical_status") {
      setLexicalStatus(null)
      setError(t("memory.maintenance.invalidResponse"))
      return false
    }
    setLexicalStatus(result.data.value)
    return true
  }, [onRunAction, t])

  // loadRecycleBin refreshes one version-guarded recycle view for the active filter.
  // loadRecycleBin 为当前筛选刷新一份受版本保护的回收视图。
  const loadRecycleBin = React.useCallback(async (filter: string, generation = pageGeneration.current) => {
    const payload = filter === "all" ? undefined : { vmmProjectId: Number(filter) }
    const result = await onRunAction("native_recycle_bin", payload)
    if (!mounted.current || generation !== pageGeneration.current) return false
    if (result?.data.kind !== "native_recycle_bin") {
      setRecycleBin(null)
      setError(t("memory.maintenance.invalidResponse"))
      return false
    }
    setRecycleBin(result.data.value)
    return true
  }, [onRunAction, t])

  // loadPage refreshes status, project names, and recycle batches in channel-safe sequence.
  // loadPage 按动作通道安全的顺序刷新状态、项目名称与回收批次。
  const loadPage = React.useCallback(async () => {
    if (!mounted.current) return
    // generation belongs to this complete refresh; only it may finish the visible loading state.
    // generation 归属本次完整刷新，只有该代次可以结束可见加载状态。
    const generation = ++pageGeneration.current
    setPagePending(true)
    try {
      // Independent maintenance capabilities still load when another status endpoint fails.
      // 其他状态端点失败时，独立的维护能力仍继续加载。
      const hasRetentionStatus = await loadStatus(generation)
      if (!mounted.current || generation !== pageGeneration.current) return
      const hasLexicalStatus = await loadLexicalStatus(generation)
      if (!mounted.current || generation !== pageGeneration.current) return
      const hasVectorStatus = await loadVectorStatus(generation)
      if (!mounted.current || generation !== pageGeneration.current) return
      const catalogResult = await onRunAction("native_project_catalog")
      if (!mounted.current || generation !== pageGeneration.current) return
      const hasCatalog = catalogResult?.data.kind === "native_project_catalog"
      if (catalogResult?.data.kind === "native_project_catalog") {
        setCatalog(catalogResult.data.value)
      } else {
        setCatalog(null)
        setError(t("memory.maintenance.invalidResponse"))
      }
      const hasRecycleBin = await loadRecycleBin(projectFilterRef.current, generation)
      if (!mounted.current || generation !== pageGeneration.current) return
      if (hasRetentionStatus && hasLexicalStatus && hasVectorStatus && hasCatalog && hasRecycleBin) setError(null)
    } finally {
      if (mounted.current && generation === pageGeneration.current) setPagePending(false)
    }
  }, [loadLexicalStatus, loadRecycleBin, loadStatus, loadVectorStatus, onRunAction, t])

  // changeProjectFilter reloads only recycle batches; status and catalog are independent of this filter.
  // changeProjectFilter 仅重读回收批次；状态与目录不依赖该筛选。
  // filter is the selected catalog ID or all; resolves after the corresponding read without mutations.
  // filter 为所选目录ID或all；对应只读请求完成后结束，不执行变更。
  const changeProjectFilter = async (filter: string) => {
    if (controlsBusy) return
    projectFilterRef.current = filter
    setProjectFilter(filter)
    setPagePending(true)
    // generation prevents a disposed filter request from finishing a newer page operation.
    // generation 阻止已释放筛选请求结束更新的页面操作。
    const generation = ++pageGeneration.current
    try { await loadRecycleBin(filter, generation) }
    finally { if (mounted.current && generation === pageGeneration.current) setPagePending(false) }
  }

  React.useEffect(() => {
    mounted.current = true
    // Defer only the effect launch so discarded StrictMode mounts never take the exclusive channel.
    // 仅延后副作用发起，使严格模式丢弃的挂载不占用独占通道。
    let active = true
    queueMicrotask(() => { if (active) void loadPage() })
    return () => { active = false; mounted.current = false; pageGeneration.current += 1 }
  }, [loadPage])

  // previewCleanup requests all-project impact without executing any mutation.
  // previewCleanup 请求全部项目影响，不执行任何变更。
  const previewCleanup = React.useCallback(async () => {
    // Claim the shared channel before state updates; a rejected duplicate must not finish this preview.
    // 在状态更新前认领共用通道，避免被拒绝的重复请求结束本次预览。
    if (actionLock.current) return
    actionLock.current = true
    setCleanupPending(true)
    try {
      const result = await onRunAction("native_retention_preview")
      if (!mounted.current) return
      if (result?.data.kind !== "native_retention_preview") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setCleanupPreview(result.data.value)
      setError(null)
    } finally { actionLock.current = false; if (mounted.current) setCleanupPending(false) }
  }, [onRunAction, t])

  // runCleanup confirms only the displayed server plan; a failed plan needs a new preview.
  // runCleanup 仅确认当前显示的服务端计划，失败计划需要重新预览。
  const runCleanup = React.useCallback(async () => {
    if (!cleanupPreview || actionLock.current) return
    actionLock.current = true
    setCleanupPending(true)
    try {
      const result = await onRunAction("native_retention_run", { previewId: cleanupPreview.previewId, confirmed: true })
      if (!mounted.current) return
      setCleanupPreview(null)
      if (result?.data.kind !== "native_retention_run") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      await loadPage()
    } finally {
      actionLock.current = false
      if (mounted.current) setCleanupPending(false)
    }
  }, [cleanupPreview, loadPage, onRunAction, t])

  // previewVectorRebuild obtains the exact versioned target set before asking for confirmation.
  // previewVectorRebuild 在请求确认前获取准确的版本化目标集合。
  const previewVectorRebuild = React.useCallback(async () => {
    if (actionLock.current) return
    actionLock.current = true
    setActionPending(true)
    setError(null)
    try {
      const result = await onRunAction("native_vector_rebuild_preview", undefined, { propagateError: true })
      if (!mounted.current || !result) return
      if (result.data.kind !== "native_vector_rebuild_preview") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setVectorPreview(result.data.value)
      setVectorAdoption(null)
      setRebuildProgress(null)
      setError(null)
    } catch (cause) {
      if (mounted.current) setError(cause instanceof Error ? cause.message : String(cause))
    } finally { actionLock.current = false; if (mounted.current) setActionPending(false) }
  }, [onRunAction, t])

  // applyVectorRebuild commits the exact previewed generation through bounded durable batches.
  // applyVectorRebuild 通过有界耐久批次提交准确预检的向量代次。
  const applyVectorRebuild = React.useCallback(async () => {
    if (!vectorPreview || actionLock.current) return
    actionLock.current = true
    setActionPending(true)
    setError(null)
    const commandId = crypto.randomUUID()
    setRebuildProgress({ rebuiltMemoryCount: 0, batchCount: 0, retryAfterSeconds: null })
    setActiveRebuildId(commandId)
    try {
      const result = await onRunAction("native_vector_rebuild", {
        expectedStorageVersion: vectorPreview.storageVersion,
        expectedPreviewFingerprint: vectorPreview.previewFingerprint,
        confirmed: true,
      }, { propagateError: true, commandId })
      if (!mounted.current || !result) return
      if (result.data.kind !== "native_vector_rebuild") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setVectorPreview(null)
      setVectorAdoption(null)
      await loadPage()
    } catch (cause) {
      if (mounted.current) setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      actionLock.current = false
      if (mounted.current) {
        setActiveRebuildId(null)
        setActionPending(false)
      }
    }
  }, [loadPage, onRunAction, t, vectorPreview])

  // applyVectorAdoption keeps same-dimension vectors after the service re-embeds one sample to prove
  // the current route runs the same model; a refused sample leaves the rebuild dialog open.
  // applyVectorAdoption 在服务重新嵌入一条抽样、证明当前路由运行同一模型后沿用同维度向量；
  // 抽样未通过时保留重建弹窗。
  const applyVectorAdoption = React.useCallback(async () => {
    if (!vectorPreview || actionLock.current) return
    actionLock.current = true
    setActionPending(true)
    setError(null)
    try {
      const result = await onRunAction("native_vector_adoption", {
        expectedStorageVersion: vectorPreview.storageVersion,
        expectedPreviewFingerprint: vectorPreview.previewFingerprint,
        confirmed: true,
      }, { propagateError: true })
      if (!mounted.current || !result) return
      if (result.data.kind !== "native_vector_adoption") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setVectorAdoption(result.data.value)
      if (!result.data.value.adopted) return
      setVectorPreview(null)
      await loadPage()
    } catch (cause) {
      if (mounted.current) setError(cause instanceof Error ? cause.message : String(cause))
    } finally { actionLock.current = false; if (mounted.current) setActionPending(false) }
  }, [loadPage, onRunAction, t, vectorPreview])

  // applyRecycleAction restores or permanently purges one exact versioned batch.
  // applyRecycleAction 恢复或永久清除一个准确的带版本批次。
  const applyRecycleAction = React.useCallback(async () => {
    if (!confirmation || actionLock.current) return
    actionLock.current = true
    setActionPending(true)
    try {
      const action = confirmation.action === "restore" ? "native_recycle_restore" : "native_recycle_purge"
      const result = await onRunAction(action, {
        batchId: confirmation.batch.batchId,
        storageVersion: confirmation.storageVersion,
      })
      if (!mounted.current) return
      if (result?.data.kind !== "native_recycle_batch") {
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setConfirmation(null)
      await loadPage()
    } finally { actionLock.current = false; if (mounted.current) setActionPending(false) }
  }, [confirmation, loadPage, onRunAction, t])

  const projectNames = React.useMemo(() => new Map(
    (catalog?.projects ?? []).map((entry) => [entry.projectId, entry.name]),
  ), [catalog])

  return (
    <div className="vk-form-loose">
      <PageTabs
        actions={<>
          <Button ref={cleanupTrigger} disabled={controlsBusy || status?.running} onClick={() => void previewCleanup()} size="sm" type="button"><Sparkles className="vk-icon" aria-hidden="true" />{t("memory.maintenance.previewCleanup")}</Button>
          <Button disabled={controlsBusy} onClick={() => void loadPage()} size="sm" type="button" variant="outline"><RefreshCw className="vk-icon" aria-hidden="true" />{t("memory.resources.refresh")}</Button>
        </>}
        ariaLabel={t("memory.maintenance.title")}
        className="vmm-page-tabs"
        onValueChange={setMaintenanceView}
        options={([["recycle", "memory.maintenance.recycleBin"], ["cleanup", "memory.workspace.cleanup"], ["index", "memory.workspace.index"]] as const).map(([value, label]) => ({ label: t(label), value }))}
        value={maintenanceView}
      />
      {error ? <p className="vk-field-error">{error}</p> : null}
      {vectorStatus && (!vectorStatus.configured || vectorStatus.rebuildRequiredMemoryCount > 0) ? (
        <div className="vk-form-alert" data-tone="bad" role="alert">
          <div className="vk-form-inline">
            <AlertTriangle aria-hidden="true" />
            <div>
              <p className="vk-form-strong">{t(vectorStatus.configured ? "memory.maintenance.vectorRebuildRequired" : "memory.maintenance.vectorRouteUnavailable")}</p>
              {vectorStatus.configured
                ? <p className="vk-form-note">{t("memory.maintenance.vectorRebuildCounts").replace("{memories}", String(vectorStatus.rebuildRequiredMemoryCount)).replace("{active}", String(vectorStatus.activeRebuildRequiredMemoryCount)).replace("{recycled}", String(vectorStatus.recycleBinRebuildRequiredMemoryCount)).replace("{spaces}", String(vectorStatus.legacyVectorSpaceCount))}</p>
                : nativeVectorRouteCause(vectorStatus) ? <p className="vk-field-error">{nativeVectorRouteCause(vectorStatus)}</p> : null}
            </div>
          </div>
          <Button onClick={() => setMaintenanceView("index")} size="sm" type="button" variant="outline">{t(vectorStatus.configured ? "memory.vectorHealth.openRepair" : "memory.maintenance.vectorTitle")}</Button>
        </div>
      ) : null}

      <div hidden={maintenanceView !== "cleanup"}><Card>
        <CardHeader><CardTitle className="vk-form-inline"><ShieldCheck className="vk-icon" aria-hidden="true" />{t("memory.maintenance.status")}</CardTitle><CardDescription>{t("memory.maintenance.statusDescription")}</CardDescription></CardHeader>
        <CardContent className="vk-form-loose">
      {status ? <><div className="vk-mem-tiles"><Metric label={t("memory.maintenance.worker")} value={status.running ? t("memory.maintenance.running") : status.enabled ? t("memory.maintenance.enabled") : t("memory.maintenance.disabled")} /><Metric label={t("memory.maintenance.profileExpiryWorker")} value={status.profileExpiryRunning ? t("memory.maintenance.running") : status.enabled ? t("memory.maintenance.enabled") : t("memory.maintenance.disabled")} /><Metric label={t("memory.maintenance.interval")} value={`${status.intervalMinutes} min`} /><Metric label={t("memory.maintenance.profileExpiryInterval")} value={`${status.profileExpiryIntervalSeconds} s`} /><Metric label={t("memory.maintenance.pendingJobs")} value={String(status.pendingColdJobs)} /><Metric label={t("memory.maintenance.recycleBatches")} value={String(status.recycleBatchCount)} /></div><div className="vk-mem-tiles"><Metric label={t("memory.maintenance.hotWindow")} value={String(status.policy.turnHotWindowSize)} /><Metric label={t("memory.maintenance.idleWindow")} value={formatNativeDuration(status.policy.sessionIdleMs, locale, t)} /><Metric label={t("memory.maintenance.trashWindow")} value={formatNativeDuration(status.policy.trashRetentionMs, locale, t)} /><Metric label={t("memory.maintenance.protectionFloor")} value={`${status.policy.protectedPriorityFloor} / L${status.policy.protectedMemoryLevelFloor}`} /></div><div className="vk-form-stack"><p className="vk-form-strong">{t("memory.maintenance.limits")}</p><div className="vk-mem-three"><Metric label={t("memory.maintenance.profileExpiryLimit")} value={String(status.policy.profileExpiryLimit)} /><Metric label={t("memory.maintenance.memoryRecycleLimit")} value={String(status.policy.memoryRecycleLimit)} /><Metric label={t("memory.maintenance.coldScanLimit")} value={String(status.policy.coldTurnScanLimit)} /><Metric label={t("memory.maintenance.coldJobLimit")} value={String(status.policy.coldTurnJobLimit)} /><Metric label={t("memory.maintenance.idleSessionLimit")} value={String(status.policy.idleSessionLimit)} /><Metric label={t("memory.maintenance.trashPurgeLimit")} value={String(status.policy.trashPurgeLimit)} /></div><p className="vk-form-note">{t("memory.maintenance.vectorConsistency")}</p></div><div className="vk-mem-two"><div className="vk-form-well"><p>{t("memory.maintenance.lastRun")}: {formatTimestamp(status.lastFinishedAtUnixMs, t("memory.maintenance.never"), locale)}</p>{status.lastResult ? <p className="vk-faint">{t("memory.maintenance.lastResult").replace("{profiles}", String(status.lastResult.expiredProfileNodes)).replace("{memories}", String(status.lastResult.recycledMemories)).replace("{turns}", String(status.lastResult.recycledTurns)).replace("{enqueued}", String(status.lastResult.enqueuedColdJobs)).replace("{completed}", String(status.lastResult.completedColdJobs)).replace("{purged}", String(status.lastResult.purgedTrashBatches))}</p> : null}{status.lastError ? <p className="vk-field-error">{status.lastError}</p> : null}{status.lastResult?.projectFailures.length ? <ul className="vk-field-error vk-form-tight vk-form-after">{status.lastResult.projectFailures.map((failure) => <li key={`${failure.projectId}:${failure.error}`}>{projectNames.get(failure.projectId) ?? `#${failure.projectId}`}: {failure.error}</li>)}</ul> : null}</div><div className="vk-form-well"><p>{t("memory.maintenance.profileExpiryLastRun")}: {formatTimestamp(status.profileExpiryLastFinishedAtUnixMs, t("memory.maintenance.never"), locale)}</p>{status.profileExpiryLastResult ? <p className="vk-faint">{t("memory.maintenance.profileExpiryLastResult").replace("{profiles}", String(status.profileExpiryLastResult.expiredProfileNodes))}</p> : null}{status.profileExpiryLastError ? <p className="vk-field-error">{status.profileExpiryLastError}</p> : null}{status.profileExpiryLastResult?.projectFailures.length ? <ul className="vk-field-error vk-form-tight vk-form-after">{status.profileExpiryLastResult.projectFailures.map((failure) => <li key={`${failure.projectId}:${failure.error}`}>{projectNames.get(failure.projectId) ?? `#${failure.projectId}`}: {failure.error}</li>)}</ul> : null}</div></div></> : pagePending ? <StreamPlaceholder label={t("memory.resources.loading")} shape="tiles" count={6} /> : <p className="vk-form-text">{t("memory.maintenance.invalidResponse")}</p>}
        </CardContent>
      </Card>

      </div>
      <div hidden={maintenanceView !== "index"}><Card>
        <CardHeader>
          <CardTitle className="vk-form-inline"><DatabaseZap className="vk-icon" aria-hidden="true" />{t("memory.maintenance.vectorTitle")}</CardTitle>
          <CardDescription>{t("memory.maintenance.vectorDescription")}</CardDescription>
        </CardHeader>
        <CardContent className="vk-form-loose">
          {vectorStatus ? <>
            <div className="vk-mem-metrics">
              <Metric label={t("memory.maintenance.vectorConfigured")} value={vectorStatus.configured ? t("memory.maintenance.configured") : t("memory.maintenance.notConfigured")} />
              <Metric label={t("memory.maintenance.vectorDimension")} value={vectorStatus.embeddingDimension == null ? "-" : String(vectorStatus.embeddingDimension)} />
              <Metric label={t("memory.maintenance.activeMemories")} value={String(vectorStatus.activeMemoryCount)} />
              <Metric label={t("memory.maintenance.recycleBinMemories")} value={String(vectorStatus.recycleBinMemoryCount)} />
              <Metric label={t("memory.maintenance.currentVectors")} value={String(vectorStatus.currentMemoryCount)} />
              <Metric label={t("memory.maintenance.currentRecycleBinVectors")} value={String(vectorStatus.currentRecycleBinMemoryCount)} />
              <Metric label={t("memory.maintenance.pendingAnalysisJobs")} value={String(vectorStatus.pendingAnalysisJobCount)} />
            </div>
            <div className="vk-form-alert" data-tone={!vectorStatus.configured || vectorStatus.rebuildRequiredMemoryCount > 0 ? "bad" : undefined}>
              <div className="vk-form-bar">
                <div>
                  <p className="vk-form-strong">{!vectorStatus.configured ? t("memory.maintenance.vectorRouteUnavailable") : vectorStatus.rebuildRequiredMemoryCount > 0 ? t("memory.maintenance.vectorRebuildRequired") : t("memory.maintenance.vectorCompatible")}</p>
                  <p className="vk-form-note">{t("memory.maintenance.vectorRebuildCounts").replace("{memories}", String(vectorStatus.rebuildRequiredMemoryCount)).replace("{active}", String(vectorStatus.activeRebuildRequiredMemoryCount)).replace("{recycled}", String(vectorStatus.recycleBinRebuildRequiredMemoryCount)).replace("{spaces}", String(vectorStatus.legacyVectorSpaceCount))}</p>
                  {nativeVectorRouteCause(vectorStatus) ? <p className="vk-field-error">{nativeVectorRouteCause(vectorStatus)}</p> : null}
                </div>
                <Button ref={vectorTrigger} disabled={controlsBusy || !vectorStatus.configured || vectorStatus.rebuildRequiredMemoryCount === 0} onClick={() => void previewVectorRebuild()} type="button" variant="outline">
                  <DatabaseZap className="vk-icon" aria-hidden="true" />{t("memory.maintenance.previewVectorRebuild")}
                </Button>
              </div>
            </div>
            {vectorAdoption?.adopted ? (
              <p className="vk-form-text" role="status">{formatSkillText(t("memory.maintenance.keepVectorsDone"), {
                count: vectorAdoption.adoptedMemoryCount,
                remaining: vectorAdoption.remainingRebuildMemoryCount,
              })}</p>
            ) : null}
          </> : pagePending ? <StreamPlaceholder label={t("memory.resources.loading")} shape="fields" count={3} /> : <p className="vk-form-text">{t("memory.maintenance.readFailed")}</p>}
        </CardContent>
      </Card>

      <VmmNativeLexicalIndex status={lexicalStatus} loading={pagePending} busy={controlsBusy} onRunAction={onRunAction} onRefresh={loadLexicalStatus} />
      </div>
      <div hidden={maintenanceView !== "recycle"}><Card>
        <CardHeader><CardTitle>{t("memory.maintenance.recycleBin")}</CardTitle><CardDescription>{t("memory.maintenance.recycleDescription")}</CardDescription></CardHeader>
        <CardContent className="vk-form-loose">
          <div className="vk-field vk-form-narrow"><Label htmlFor="memory-recycle-project-filter">{t("memory.maintenance.projectFilter")}</Label><Select disabled={controlsBusy} onValueChange={(filter) => void changeProjectFilter(filter)} value={projectFilter}><SelectTrigger id="memory-recycle-project-filter"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">{t("memory.maintenance.allProjects")}</SelectItem>{catalog?.projects.map((project) => <SelectItem key={project.projectId} value={String(project.projectId)}>{project.name} · {project.tag}</SelectItem>)}</SelectContent></Select></div>
          {recycleBin?.batches.length ? <TooltipProvider><ol className="vmm-recycle-list" aria-label={t("memory.maintenance.recycleBin")}>{recycleBin.batches.map((batch) => <RecycleBatchCard batch={batch} busy={controlsBusy} key={batch.batchId} onRunAction={onRunAction} onConfirm={(action, trigger) => { recycleTrigger.current = trigger; setError(null); setConfirmation({ action, batch, storageVersion: recycleBin.storageVersion }) }} projectName={projectNames.get(batch.projectId) ?? `#${batch.projectId}`} />)}</ol></TooltipProvider> : recycleBin === null && !error ? <StreamPlaceholder label={t("memory.common.loading")} shape="cards" count={3} /> : <div className="vk-mem-placeholder">{recycleBin !== null ? t("memory.maintenance.emptyRecycleBin") : t("memory.maintenance.invalidResponse")}</div>}
        </CardContent>
      </Card>

      </div>
      <AlertDialog open={confirmation !== null} onOpenChange={(open) => !open && !busy && !actionLock.current && setConfirmation(null)}><AlertDialogContent onCloseAutoFocus={(event) => { event.preventDefault(); recycleTrigger.current?.focus() }} onEscapeKeyDown={(event) => { if (actionLock.current || busy) event.preventDefault() }}><AlertDialogHeader><AlertDialogTitle>{confirmation?.action === "purge" ? t("memory.maintenance.confirmPurgeTitle") : t("memory.maintenance.confirmRestoreTitle")}</AlertDialogTitle><AlertDialogDescription>{confirmation?.action === "purge" ? t("memory.maintenance.confirmPurgeDescription") : t("memory.maintenance.confirmRestoreDescription")}</AlertDialogDescription></AlertDialogHeader>{confirmation ? <div className="vk-form-tight vk-form-sm"><p className="vk-form-break vk-form-em">{projectNames.get(confirmation.batch.projectId) ?? `#${confirmation.batch.projectId}`}</p><p className="vk-form-codeline vk-faint">{confirmation.batch.batchId}</p></div> : null}{error ? <p role="alert" className="vk-field-error">{error}</p> : null}<AlertDialogFooter><AlertDialogCancel disabled={busy || actionPending}>{t("memory.common.cancel")}</AlertDialogCancel><AlertDialogAction disabled={busy || actionPending} onClick={(event) => { event.preventDefault(); void applyRecycleAction() }}>{confirmation?.action === "purge" ? t("memory.maintenance.purge") : t("memory.maintenance.restore")}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
      <VmmNativeCleanupPreview preview={cleanupPreview} busy={busy || cleanupPending} onClose={() => { if (!busy && !actionLock.current) setCleanupPreview(null) }} onConfirm={() => void runCleanup()} onRestoreFocus={() => cleanupTrigger.current?.focus()} />
      <AlertDialog open={vectorPreview !== null} onOpenChange={(open) => !open && !busy && !actionLock.current && setVectorPreview(null)}>
        <AlertDialogContent onCloseAutoFocus={(event) => { event.preventDefault(); vectorTrigger.current?.focus() }} onEscapeKeyDown={(event) => { if (actionLock.current || busy) event.preventDefault() }}>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("memory.maintenance.confirmVectorRebuildTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {vectorPreview ? t("memory.maintenance.confirmVectorRebuildDescription")
                .replace("{memories}", String(vectorPreview.rebuildMemoryCount))
                .replace("{active}", String(vectorPreview.activeRebuildMemoryCount))
                .replace("{recycled}", String(vectorPreview.recycleBinRebuildMemoryCount))
                .replace("{projects}", String(vectorPreview.affectedProjectCount))
                .replace("{batches}", String(vectorPreview.batchCount))
                .replace("{dimension}", String(vectorPreview.embeddingDimension)) : ""}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {vectorPreview && vectorPreview.adoptableMemoryCount > 0 ? (
            <div className="vk-form-well vk-form-tight">
              <p className="vk-form-em">{t("memory.maintenance.keepVectorsTitle")}</p>
              <p className="vk-form-note">{formatSkillText(t("memory.maintenance.keepVectorsDescription"), { count: vectorPreview.adoptableMemoryCount })}</p>
              {vectorAdoption && !vectorAdoption.adopted ? (
                <p role="alert" className="vk-field-error">{formatSkillText(t("memory.maintenance.keepVectorsRejected"), {
                  similarity: vectorAdoption.sampleSimilarity.toFixed(4),
                  required: vectorAdoption.requiredSimilarity.toFixed(4),
                })}</p>
              ) : null}
            </div>
          ) : null}
          {vectorPreview && rebuildProgress ? (
            <VectorRebuildProgress preview={vectorPreview} progress={rebuildProgress} running={activeRebuildId !== null} />
          ) : null}
          {error ? <p role="alert" className="vk-field-error vk-form-break">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy || actionPending}>{t("memory.common.cancel")}</AlertDialogCancel>
            {vectorPreview && vectorPreview.adoptableMemoryCount > 0 && !(vectorAdoption && !vectorAdoption.adopted) ? (
              <Button disabled={busy || actionPending} onClick={() => void applyVectorAdoption()} type="button" variant="outline">{t("memory.maintenance.keepVectors")}</Button>
            ) : null}
            <AlertDialogAction disabled={busy || actionPending} onClick={(event) => { event.preventDefault(); void applyVectorRebuild() }}>{t("memory.maintenance.rebuildVectors")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

// Metric renders one compact retention status value.
// Metric 渲染一个紧凑的保留状态值。
// Parameters: label names the setting; value contains the formatted status and unit. Returns: a compact definition row.
// 参数：label 为设置名称；value 包含已格式化状态和单位。返回值：紧凑的定义行。
function Metric({ label, value }: { label: string; value: string }) {
  return <div className="vk-mem-metric"><p>{label}</p><p className="vk-num">{value}</p></div>
}

// RecycleBatchCard renders one recoverable batch as a list row: identity and contents on the left, guarded actions on the right.
// Storage identifiers stay behind a marker tooltip; only non-empty record families are listed.
// RecycleBatchCard 将一个可恢复批次渲染为列表行：左侧为身份与内容，右侧为受保护动作。
// 存储标识收进标记的提示中；只列出非空的记录类别。
function RecycleBatchCard({ batch, busy, onConfirm, projectName, onRunAction }: { batch: VmmNativeRecycleBatchSummary; busy: boolean; onConfirm: (action: "restore" | "purge", trigger: HTMLButtonElement) => void; projectName: string; onRunAction: VmmNativeMaintenancePageProps["onRunAction"] }) {
  const { t, locale } = useI18n()
  const separator = t("global.labelSeparator")
  const counts = ([
    ["memory.maintenance.turns", batch.turnCount],
    ["memory.maintenance.memories", batch.memoryCount],
    ["memory.maintenance.profiles", batch.profileNodeCount],
    ["memory.maintenance.evidence", batch.contextEvidenceCount],
  ] as const).filter(([, count]) => count > 0)
  const expiresAt = formatTimestamp(batch.expiresAtUnixMs, "-", locale)
  return <li className="vmm-recycle-batch">
    <div className="vmm-recycle-batch-main">
      <div className="vmm-recycle-batch-title">
        <span className="vmm-recycle-batch-name" title={projectName}>{projectName}</span>
        <span className="vmm-recycle-batch-reason">{retentionReasonLabel(batch.reason, t)}</span>
        <Tooltip>
          <TooltipTrigger asChild>
            <button aria-label={`${t("memory.maintenance.batchId")}${separator}${batch.batchId}`} className="vmm-recycle-batch-marker" type="button"><Fingerprint className="vk-icon" aria-hidden="true" /></button>
          </TooltipTrigger>
          <TooltipContent className="vk-mem-col-start" side="top">
            <span>{t("memory.maintenance.batchId")}</span>
            <span className="vk-mono">{batch.batchId}</span>
            <span>{t("memory.maintenance.recycledAt")}{separator}{formatTimestamp(batch.recycledAtUnixMs, "-", locale)}</span>
          </TooltipContent>
        </Tooltip>
      </div>
      <div className="vmm-recycle-batch-meta">
        {counts.length > 0
          ? counts.map(([label, count]) => <span key={label}>{t(label)} <strong>{count}</strong></span>)
          : <span>{t("memory.maintenance.emptyBatch")}</span>}
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="vmm-recycle-batch-expiry" tabIndex={0}><Clock className="vk-icon" aria-hidden="true" /><span className="vk-sr-only">{t("memory.maintenance.expiresAt")}{separator}</span>{expiresAt}</span>
          </TooltipTrigger>
          <TooltipContent side="top">{t("memory.maintenance.expiresAt")}</TooltipContent>
        </Tooltip>
      </div>
    </div>
    <div className="vmm-recycle-batch-actions">
      <VmmNativeRecycleRecords batch={batch} busy={busy} onRunAction={onRunAction} />
      <Button disabled={busy} onClick={(event) => onConfirm("restore", event.currentTarget)} size="sm" type="button" variant="ghost"><ArchiveRestore className="vk-icon" aria-hidden="true" />{t("memory.maintenance.restore")}</Button>
      <Button disabled={busy} onClick={(event) => onConfirm("purge", event.currentTarget)} size="sm" type="button" variant="destructive"><Trash2 className="vk-icon" aria-hidden="true" />{t("memory.maintenance.purge")}</Button>
    </div>
    {batch.manualSelection ? <details className="vmm-recycle-batch-sources">
      <summary>{t("memory.maintenance.manualSources")} ({batch.manualSelection.ids.length})</summary>
      <p className="vk-form-note">{t(batch.manualSelection.kind === "sessions" ? "memory.removal.sessionEffect" : "memory.removal.turnEffect")}</p>
      <ul className="vk-mem-scroll-list vk-form-after">{batch.manualSelection.ids.map((id) => <li className="vk-form-codeline" key={id}>{id}</li>)}</ul>
    </details> : null}
  </li>
}

// retentionReasonLabel localizes one closed Core retention reason token.
// retentionReasonLabel 本地化一个封闭 Core 保留原因标记。
function retentionReasonLabel(reason: string, t: ReturnType<typeof useI18n>["t"]): string {
  if (reason === "terminal_memory_recycle") return t("memory.maintenance.reasonTerminal")
  if (reason === "cold_turn_recycle") return t("memory.maintenance.reasonColdTurn")
  if (reason === "idle_session_recycle") return t("memory.maintenance.reasonIdle")
  if (reason === "manual_session_recycle") return t("memory.maintenance.reasonManualSession")
  if (reason === "manual_turn_recycle") return t("memory.maintenance.reasonManualTurn")
  return reason
}

// formatTimestamp renders an optional Unix millisecond timestamp without inventing a value.
// formatTimestamp 渲染可选 Unix 毫秒时间戳，且不虚构值。
function formatTimestamp(value: number | null | undefined, fallback: string, locale: string): string {
  return value == null ? fallback : formatDateTime(value, locale) ?? fallback
}
