import * as React from "react"
import { Button } from "@/components/ui/button"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog"
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
  const eligible = sessions.length > 0 && sessions.length <= 100 && sessions.every((session) => session.status === "forgotten")

  // Requests a read-only preview for stable IDs; returns without changing lifecycle state.
  // 为稳定 ID 请求只读预览，不改变生命周期状态。
  const prepare = async () => {
    if (!eligible || busy || pending) return
    setPending(true); onBusyChange(true); setError(false)
    try {
      const result = await onRunAction("native_session_restart_preview", { vmmProjectId: projectId, sessionSourceIds: sessions.map((session) => session.sessionSourceId) })
      if (result?.data?.kind === "native_session_restart_preview") setPreview(result.data.value)
      else setError(true)
    } catch { setError(true) }
    finally { setPending(false); onBusyChange(false) }
  }

  // Confirms only the opaque preview; failed execution requires a fresh preview and never claims success.
  // 只确认不透明预览；执行失败需要重新预览，不能宣称成功。
  const confirm = async () => {
    if (!preview || busy || pending) return
    setPending(true); onBusyChange(true); setError(false)
    try {
      const result = await onRunAction("native_session_restart", { previewId: preview.previewId, confirmed: true })
      setPreview(null)
      if (result?.data?.kind === "native_session_restart") await onRefresh()
      else setError(true)
    } catch { setPreview(null); setError(true) }
    finally { setPending(false); onBusyChange(false) }
  }

  if (!eligible) return null
  return <div className="space-y-2">
    <Button type="button" variant="outline" disabled={busy || pending} onClick={() => void prepare()}>{t("memory.sessionRestart.action")}</Button>
    {error ? <p role="alert" className="text-sm text-destructive">{t("memory.sessionRestart.failed")}</p> : null}
    <AlertDialog open={preview !== null} onOpenChange={(open) => { if (!open && !pending) setPreview(null) }}>
      <AlertDialogContent>
        <AlertDialogHeader><AlertDialogTitle>{t("memory.sessionRestart.action")}</AlertDialogTitle><AlertDialogDescription>{t("memory.sessionRestart.description")}</AlertDialogDescription></AlertDialogHeader>
        <ul className="max-h-64 space-y-2 overflow-y-auto">{preview?.selection.sessionSourceIds.map((id) => <li key={id}>
          <p className="break-all text-sm">{sessions.find((session) => session.sessionSourceId === id)?.displayTitle ?? id}</p>
          <p className="break-all font-mono text-xs text-muted-foreground">{id}</p>
        </li>)}</ul>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy || pending}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={busy || pending || !preview} onClick={(event) => { event.preventDefault(); void confirm() }}>{t("memory.sessionRestart.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
}
