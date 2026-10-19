import * as React from "react"
import { Trash2 } from "lucide-react"
import { Button } from "@/ui-kit/primitives/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger } from "@/ui-kit/primitives/alert-dialog"
import { useI18n } from "@/i18n"
import { formatNativeDuration } from "./native-duration-display"
import type { VmmMemoryActionResult, VmmNativeRemovalPreview, VmmNativeRemovalTarget } from "@/types"

// Binds a selected source family to the existing native preview/confirmation transport.
// 将所选来源类别绑定到既有原生预览及确认传输。
interface RemovalProps {
  // Stable owner and canonical identities; titles never participate in mutation.
  // 稳定归属及规范身份；标题绝不参与变更。
  projectId: number
  selection: VmmNativeRemovalTarget
  // Display names are optional because the authoritative preview may include other source sessions.
  // 显示名称可选，因为权威预览可能包含其他来源会话。
  sessionNames: ReadonlyMap<string, string>
  // Parent transport occupancy and callback prevent overlapping actions while awaiting responses.
  // 父级传输占用及回调防止等待响应期间动作重叠。
  busy: boolean
  onBusyChange: (busy: boolean) => void
  // Returns the typed action result; refresh reloads source records only after a successful move.
  // 返回类型化动作结果；刷新仅在成功移动后重新读取来源记录。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onRefresh: () => Promise<void>
}

// Renders read-only impact and explicit consent; inputs are a fixed selection, output is host actions.
// 展示只读影响及明确同意；输入为固定选择，输出为宿主动作。
export function VmmNativeRemoval({ projectId, selection, sessionNames, busy, onBusyChange, onRunAction, onRefresh }: RemovalProps) {
  // Preview is server-owned and never reconstructed from client-side counts.
  // 预览由服务端拥有，绝不根据客户端计数重建。
  const [preview, setPreview] = React.useState<VmmNativeRemovalPreview | null>(null)
  // Pending state drives the view; a synchronous latch also blocks same-tick double submission.
  // 等待状态驱动视图；同步锁同时阻止同一事件周期重复提交。
  const [pending, setPending] = React.useState(false)
  const inFlight = React.useRef(false)
  // Failed or consumed plans cannot be confirmed again without a fresh preview.
  // 失败或已消费的计划必须重新预览后才能确认。
  const [error, setError] = React.useState(false)
  // A changed parent selection invalidates late responses, including changes away and back.
  // 父级选择变化会失效迟到响应，包括切走后再次切回。
  const identity = JSON.stringify([projectId, selection.kind, selection.ids])
  const identityRef = React.useRef(identity)
  const generationRef = React.useRef(0)
  if (identityRef.current !== identity) {
    identityRef.current = identity
    generationRef.current += 1
  }
  // Tracks the selection that owns the visible preview without silently retargeting it.
  // 跟踪可见预览所属选择，不静默替换其目标。
  const previewGeneration = React.useRef(-1)
  const mounted = React.useRef(true)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  // Feedback belongs to the selected owner and source generation, just like the preview.
  // 反馈与预览一样归属于选定项目和来源代次，切换选择后清除旧错误。
  React.useEffect(() => { setError(false) }, [identity])
  // Translation and eligibility are presentation-only; the server validates the full selection again.
  // 翻译及可选状态仅用于展示；服务端会再次校验完整选择。
  const { t, locale } = useI18n()
  const eligible = selection.ids.length > 0 && selection.ids.length <= 100
  const visiblePreview = previewGeneration.current === generationRef.current ? preview : null

  // Requests a read-only plan for the exact current selection; returns no storage mutation.
  // 为准确当前选择请求只读计划，不产生存储变更。
  const prepare = async () => {
    if (busy || inFlight.current || !eligible) return
    inFlight.current = true
    setPending(true)
    setError(false)
    setPreview(null)
    onBusyChange(true)
    // Capture the admitted selection generation before dispatching the asynchronous read.
    // 在派发异步读取前捕获已接纳的选择代次。
    const generation = generationRef.current
    try {
      const result = await onRunAction("native_removal_preview", { vmmProjectId: projectId, selection })
      if (!mounted.current || generationRef.current !== generation) return
      if (result?.data.kind === "native_removal_preview") {
        previewGeneration.current = generation
        setPreview(result.data.value)
      } else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) setError(true) }
    finally {
      inFlight.current = false
      if (mounted.current) setPending(false)
      onBusyChange(false)
    }
  }

  // Confirms only the displayed opaque plan; failure consumes the local confirmation opportunity.
  // 仅确认显示的不透明计划；失败后取消本地再次确认机会。
  const confirm = async () => {
    if (busy || inFlight.current || !visiblePreview) return
    inFlight.current = true
    setPending(true)
    setError(false)
    onBusyChange(true)
    // Keep refresh bound to the admitted source view, never a newly selected project.
    // 将刷新绑定到已接纳来源视图，绝不刷新新选中的项目。
    const generation = generationRef.current
    try {
      const result = await onRunAction("native_removal_recycle", { previewId: visiblePreview.previewId, confirmed: true })
      if (!mounted.current || generationRef.current !== generation) return
      setPreview(null)
      if (result?.data.kind === "native_removal") await onRefresh()
      else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) { setPreview(null); setError(true) } }
    finally {
      inFlight.current = false
      if (mounted.current) setPending(false)
      onBusyChange(false)
    }
  }

  return <AlertDialog open={visiblePreview !== null} onOpenChange={(open) => { if (!open && !inFlight.current) setPreview(null) }}><div className="space-y-2">
    <AlertDialogTrigger asChild><Button type="button" variant="outline" disabled={busy || pending || !eligible} onClick={() => void prepare()}>
      <Trash2 className="size-4" aria-hidden="true" />
      {t(selection.kind === "sessions" ? "memory.removal.sessions" : "memory.removal.turns")} ({selection.ids.length})
    </Button></AlertDialogTrigger>
    {error ? <p role="alert" className="text-sm text-destructive">{t("memory.removal.failed")}</p> : null}
      <AlertDialogContent className="max-h-[85vh] overflow-y-auto">
        <AlertDialogHeader>
          <AlertDialogTitle>{t("memory.removal.title")}</AlertDialogTitle>
          <AlertDialogDescription>{t("memory.removal.description")}</AlertDialogDescription>
        </AlertDialogHeader>
        {visiblePreview ? <>
          <p className="text-sm">{t(visiblePreview.impact.query.selection.kind === "sessions" ? "memory.removal.sessionEffect" : "memory.removal.turnEffect")}</p>
          <dl className="grid grid-cols-2 gap-2 text-sm">
            <RemovalCount label={t("memory.nativeRecords.sessions")} count={visiblePreview.impact.sessionSourceIds.length} />
            <RemovalCount label={t("memory.nativeRecords.turns")} count={visiblePreview.impact.turnIds.length} />
            <RemovalCount label={t("memory.nativeRecords.memories")} count={visiblePreview.impact.memoryIds.length} />
            <RemovalCount label={t("memory.removal.vectors")} count={visiblePreview.impact.vectorMemoryIds.length} />
            <RemovalCount label={t("memory.nativeRecords.pending")} count={visiblePreview.impact.pendingTurnIds.length} />
            <RemovalCount label={t("memory.removal.hotTurns")} count={visiblePreview.impact.hotWindowTurnIds.length} />
            <RemovalCount label={t("memory.removal.context")} count={visiblePreview.impact.contextEvidence.length} />
            <RemovalCount label={t("memory.removal.hotWindow")} count={visiblePreview.impact.hotWindowSize} />
            <RemovalCount label={t("memory.removal.projectProfiles")} count={visiblePreview.impact.projectProfileNodeIds.length} />
            <RemovalCount label={t("memory.removal.userProfiles")} count={visiblePreview.impact.userProfileNodeIds.length} />
          </dl>
          <p className="text-sm">{t("memory.removal.retention")} {formatNativeDuration(visiblePreview.trashRetentionMs, locale, t)}{t("memory.removal.expirySuffix")}</p>
          <details className="rounded-md border p-3 text-sm">
            <summary className="cursor-pointer">{t("memory.removal.sources")}</summary>
            <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto">
              {visiblePreview.impact.sessionSourceIds.map((id) => <li className="break-all" key={id}>{sessionNames.get(id) ?? id}<span className="ml-2 font-mono text-xs text-muted-foreground">{id}</span></li>)}
              {visiblePreview.impact.turnIds.map((id) => <li className="break-all font-mono text-xs" key={id}>{id}</li>)}
            </ul>
          </details>
        </> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={pending}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={busy || pending || !visiblePreview} onClick={(event) => { event.preventDefault(); void confirm() }}>{t("memory.removal.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
  </div></AlertDialog>
}

// Renders one authoritative impact count from its translated label and numeric value.
// 根据翻译标签及数值展示一项权威影响计数。
function RemovalCount({ label, count }: { label: string; count: number }) {
  return <div className="rounded-md border p-2"><dt className="text-muted-foreground">{label}</dt><dd>{count}</dd></div>
}
