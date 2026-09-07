import * as React from "react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { VmmNativeSessionRestart } from "./vmm-native-session-restart"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog"
import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary, VmmNativeSessionArchivePreview } from "@/types"

// Status labels describe authoritative metadata; null is explicitly not a lifecycle state.
// 状态文案描述权威元数据；null 明确不属于生命周期状态。
const statusKeys: Record<NonNullable<VmmNativeMemorySessionSummary["status"]>, TranslationKey> = {
  active: "memory.sessionArchive.active", archived: "memory.sessionArchive.archived",
  recycled: "memory.sessionArchive.recycled", forgotten: "memory.sessionArchive.forgotten",
}

// Renders the persisted status, taking a nullable native state and returning its user-facing badge.
// 接收可空原生状态并返回用户状态徽标，展示持久化状态。
export function NativeSessionStatus({ status }: { status: VmmNativeMemorySessionSummary["status"] }) {
  const { t } = useI18n()
  return <Badge variant="outline">{t(status === null ? "memory.sessionArchive.unregistered" : statusKeys[status])}</Badge>
}

// ArchiveProps binds the exact owner/session and the existing native action transport.
// ArchiveProps 绑定准确归属/会话及既有原生动作传输。
interface ArchiveProps {
  // Stable identities and current persisted status, never display-name bindings.
  // 稳定身份及当前持久化状态，不使用显示名称绑定。
  projectId: number
  sessions: VmmNativeMemorySessionSummary[]
  // Parent occupancy prevents changing selection during an operation.
  // 父级占用状态防止操作期间改变选择。
  busy: boolean
  onBusyChange: (busy: boolean) => void
  // Returns the typed host action result; refresh reads the accepted state after success.
  // 返回类型化宿主动作结果；刷新在成功后读取已接纳状态。
  onRunAction: (action: string, payload?: Record<string, unknown>) => Promise<VmmMemoryActionResult | null>
  onRefresh: () => Promise<void>
}

// Previews and confirms an exact homogeneous selection; never silently drops ineligible sessions.
// 预览并确认准确且状态一致的选择；绝不静默丢弃不符合条件的会话。
export function VmmNativeSessionArchive({ projectId, sessions, busy, onBusyChange, onRunAction, onRefresh }: ArchiveProps) {
  const { t } = useI18n()
  const [preview, setPreview] = React.useState<VmmNativeSessionArchivePreview | null>(null)
  const [pending, setPending] = React.useState(false)
  const [error, setError] = React.useState(false)
  // Only these two states admit a classification change; paused and metadata-free rows cannot.
  // 只有这两种状态允许分类变更；暂停及无元数据的记录不允许。
  const allActive = sessions.length > 0 && sessions.every((session) => session.status === "active")
  const allArchived = sessions.length > 0 && sessions.every((session) => session.status === "archived")
  const eligible = sessions.length <= 100 && (allActive || allArchived)
  const actionKey = allArchived ? "memory.sessionArchive.unarchive" : "memory.sessionArchive.archive"

  // Loads a read-only server preview for the fixed current selection; returns no local mutation.
  // 为固定当前选择加载只读服务端预览，不产生本地变更。
  const prepare = async () => {
    if (busy || pending || !eligible) return
    setPending(true)
    onBusyChange(true)
    setError(false)
    try {
      const result = await onRunAction("native_session_archive_preview", {
        vmmProjectId: projectId, sessionSourceIds: sessions.map((session) => session.sessionSourceId), archived: allActive,
      })
      if (result?.data?.kind === "native_session_archive_preview") setPreview(result.data.value)
      else setError(true)
    } catch { setError(true) }
    finally { setPending(false); onBusyChange(false) }
  }

  // Sends only the opaque preview identity; a failed/consumed preview requires a new preview.
  // 只发送不透明预览身份；失败或已消费的预览必须重新生成。
  const confirm = async () => {
    if (busy || pending || !preview) return
    setPending(true)
    onBusyChange(true)
    setError(false)
    try {
      const result = await onRunAction("native_session_archive", { previewId: preview.previewId, confirmed: true })
      setPreview(null)
      if (result?.data?.kind === "native_session_archive") await onRefresh()
      else setError(true)
    } catch { setPreview(null); setError(true) }
    finally { setPending(false); onBusyChange(false) }
  }

  return <div className="space-y-2">
    <div className="flex flex-wrap items-center gap-2">
      {sessions.length === 1 ? <NativeSessionStatus status={sessions[0].status} /> : null}
      {eligible ? <Button type="button" variant="outline" disabled={busy || pending} onClick={() => void prepare()}>{t(actionKey)}</Button> : null}
      <VmmNativeSessionRestart projectId={projectId} sessions={sessions} busy={busy || pending} onBusyChange={onBusyChange} onRunAction={onRunAction} onRefresh={onRefresh} />
    </div>
    {error ? <p role="alert" className="text-sm text-destructive">{t("memory.sessionArchive.failed")}</p> : null}
    <AlertDialog open={preview !== null} onOpenChange={(open) => { if (!open && !pending) setPreview(null) }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t(preview?.selection.archived ? "memory.sessionArchive.archive" : "memory.sessionArchive.unarchive")}</AlertDialogTitle>
          <AlertDialogDescription>{t("memory.sessionArchive.description")}</AlertDialogDescription>
        </AlertDialogHeader>
        <ul className="max-h-64 space-y-2 overflow-y-auto">
          {preview?.selection.sessionSourceIds.map((sessionSourceId) => {
            // Titles are presentation only; the accepted preview identity remains authoritative.
            // 标题只用于展示；已接纳预览中的身份始终具有权威性。
            const session = sessions.find((item) => item.sessionSourceId === sessionSourceId)
            return <li key={sessionSourceId}>
              <p className="break-all text-sm">{session?.displayTitle ?? sessionSourceId}</p>
              <p className="break-all font-mono text-xs text-muted-foreground">{sessionSourceId}</p>
            </li>
          })}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy || pending}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={busy || pending || !preview} onClick={(event) => { event.preventDefault(); void confirm() }}>{t("memory.sessionArchive.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
}
