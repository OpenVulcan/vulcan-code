import * as React from "react"
import { Button } from "@/ui-kit/primitives/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger } from "@/ui-kit/primitives/alert-dialog"
import { useI18n } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary, VmmNativeSessionRestartPreview } from "@/types"

// Defines exact native targets and parent-owned operation occupancy for restart.
// 定义重新启用的准确原生目标及父级拥有的操作占用状态。
interface RestartProps {
  // Stable owner and selected persisted session records.
  // 稳定归属及选中的持久化会话记录。
  projectId: number
  sessions: VmmNativeMemorySessionSummary[]
  // Parent transport occupancy and its explicit update callback.
  // 父级传输占用状态及其明确更新回调。
  busy: boolean
  onBusyChange: (busy: boolean) => void
  // Executes an existing host action and reloads authoritative records after success.
  // 执行既有宿主动作，在成功后重载权威记录。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onRefresh: () => Promise<void>
}

// Renders a confirmation flow only for a bounded all-forgotten selection; never restores content.
// 仅为有界且全部已遗忘的选择渲染确认流程，绝不恢复内容。
export function VmmNativeSessionRestart({ projectId, sessions, busy, onBusyChange, onRunAction, onRefresh }: RestartProps) {
  const { t } = useI18n()
  // The accepted preview alone determines confirmation targets.
  // 只有已接纳预览决定确认目标。
  const [preview, setPreview] = React.useState<VmmNativeSessionRestartPreview | null>(null)
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState(false)
  // operation excludes duplicate clicks; identity invalidates a preview when the exact selection changes.
  // operation 排除重复点击；identity 在准确选择改变时使预览失效。
  const operation = React.useRef(false)
  const identity = JSON.stringify([projectId, sessions.map((session) => [session.sessionSourceId, session.status])])
  const identityRef = React.useRef(identity)
  const generationRef = React.useRef(0)
  const previewGeneration = React.useRef(-1)
  if (identityRef.current !== identity) { identityRef.current = identity; generationRef.current += 1 }
  // mounted prevents a departed page from publishing delayed confirmation feedback.
  // mounted 防止已经离开的页面发布迟到确认反馈。
  const mounted = React.useRef(true)
  React.useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  // Feedback belongs to the selected owner and source generation, just like the preview.
  // 反馈与预览一样归属于选定项目和来源代次，切换选择后清除旧错误。
  React.useEffect(() => { setError(false) }, [identity])
  const visiblePreview = previewGeneration.current === generationRef.current ? preview : null

  const eligible = sessions.length > 0 && sessions.length <= 100 && sessions.every((session) => session.status === "forgotten")

  // Requests a read-only preview for stable IDs; returns without changing lifecycle state.
  // 为稳定 ID 请求只读预览，不改变生命周期状态。
  const prepare = async () => {
    if (!eligible || busy || operation.current) return
    operation.current = true
    const generation = generationRef.current
    setPending(true); onBusyChange(true); setError(false)
    try {
      const result = await onRunAction("native_session_restart_preview", { vmmProjectId: projectId, sessionSourceIds: sessions.map((session) => session.sessionSourceId) })
      if (!mounted.current || generationRef.current !== generation) return
      if (result?.data.kind === "native_session_restart_preview") { previewGeneration.current = generation; setPreview(result.data.value) }
      else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) setError(true) }
    finally { operation.current = false; if (mounted.current) setPending(false); onBusyChange(false) }
  }

  // Confirms only the opaque preview; failed execution requires a fresh preview and never claims success.
  // 只确认不透明预览；执行失败需要重新预览，不能宣称成功。
  const confirm = async () => {
    if (!visiblePreview || busy || operation.current) return
    operation.current = true
    const generation = generationRef.current
    setPending(true); onBusyChange(true); setError(false)
    try {
      const result = await onRunAction("native_session_restart", { previewId: visiblePreview.previewId, confirmed: true })
      if (!mounted.current || generationRef.current !== generation) return
      setPreview(null)
      if (result?.data?.kind === "native_session_restart") await onRefresh()
      else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) { setPreview(null); setError(true) } }
    finally { operation.current = false; if (mounted.current) setPending(false); onBusyChange(false) }
  }

  if (!eligible) return null
  return <AlertDialog open={visiblePreview !== null} onOpenChange={(open) => { if (!open && !pending) setPreview(null) }}><div className="space-y-2">
    <AlertDialogTrigger asChild><Button type="button" variant="outline" disabled={busy || pending} onClick={() => void prepare()}>{t("memory.sessionRestart.action")}</Button></AlertDialogTrigger>
    {error ? <p role="alert" className="text-sm text-destructive">{t("memory.sessionRestart.failed")}</p> : null}
      <AlertDialogContent>
        <AlertDialogHeader><AlertDialogTitle>{t("memory.sessionRestart.action")}</AlertDialogTitle><AlertDialogDescription>{t("memory.sessionRestart.description")}</AlertDialogDescription></AlertDialogHeader>
        <ul className="max-h-64 space-y-2 overflow-y-auto">{visiblePreview?.selection.sessionSourceIds.map((id) => <li key={id}>
          <p className="break-all text-sm">{sessions.find((session) => session.sessionSourceId === id)?.displayTitle ?? id}</p>
          <p className="break-all font-mono text-xs text-muted-foreground">{id}</p>
        </li>)}</ul>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy || pending}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={busy || pending || !visiblePreview} onClick={(event) => { event.preventDefault(); void confirm() }}>{t("memory.sessionRestart.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
  </div></AlertDialog>
}
