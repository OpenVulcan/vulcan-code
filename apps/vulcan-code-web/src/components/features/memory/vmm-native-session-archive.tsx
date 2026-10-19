import * as React from "react"
import { Button } from "@/ui-kit/primitives/button"
import { Badge } from "@/ui-kit/primitives/badge"
import { VmmNativeSessionRestart } from "./vmm-native-session-restart"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger } from "@/ui-kit/primitives/alert-dialog"
import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemoryActionResult, VmmNativeMemorySessionSummary, VmmNativeSessionArchivePreview } from "@/types"
import "@/ui-kit/styles/features/settings-forms.css"
import "@/ui-kit/styles/features/memory.css"

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

  // Only these two states admit a classification change; paused and metadata-free rows cannot.
  // 只有这两种状态允许分类变更；暂停及无元数据的记录不允许。
  const allActive = sessions.length > 0 && sessions.every((session) => session.status === "active")
  const allArchived = sessions.length > 0 && sessions.every((session) => session.status === "archived")
  const eligible = sessions.length <= 100 && (allActive || allArchived)
  const actionKey = allArchived ? "memory.sessionArchive.unarchive" : "memory.sessionArchive.archive"

  // Loads a read-only server preview for the fixed current selection; returns no local mutation.
  // 为固定当前选择加载只读服务端预览，不产生本地变更。
  const prepare = async () => {
    if (busy || operation.current || !eligible) return
    operation.current = true
    const generation = generationRef.current
    setPending(true)
    onBusyChange(true)
    setError(false)
    try {
      const result = await onRunAction("native_session_archive_preview", {
        vmmProjectId: projectId, sessionSourceIds: sessions.map((session) => session.sessionSourceId), archived: allActive,
      })
      if (!mounted.current || generationRef.current !== generation) return
      if (result?.data.kind === "native_session_archive_preview") { previewGeneration.current = generation; setPreview(result.data.value) }
      else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) setError(true) }
    finally { operation.current = false; if (mounted.current) setPending(false); onBusyChange(false) }
  }

  // Sends only the opaque preview identity; a failed/consumed preview requires a new preview.
  // 只发送不透明预览身份；失败或已消费的预览必须重新生成。
  const confirm = async () => {
    if (busy || operation.current || !visiblePreview) return
    operation.current = true
    const generation = generationRef.current
    setPending(true)
    onBusyChange(true)
    setError(false)
    try {
      const result = await onRunAction("native_session_archive", { previewId: visiblePreview.previewId, confirmed: true })
      if (!mounted.current || generationRef.current !== generation) return
      setPreview(null)
      if (result?.data?.kind === "native_session_archive") await onRefresh()
      else setError(true)
    } catch { if (mounted.current && generationRef.current === generation) { setPreview(null); setError(true) } }
    finally { operation.current = false; if (mounted.current) setPending(false); onBusyChange(false) }
  }

  return <AlertDialog open={visiblePreview !== null} onOpenChange={(open) => { if (!open && !pending) setPreview(null) }}><div className="vk-form-stack">
    <div className="vk-form-inline">
      {sessions.length === 1 ? <NativeSessionStatus status={sessions[0].status} /> : null}
      {eligible ? <AlertDialogTrigger asChild><Button type="button" variant="outline" disabled={busy || pending} onClick={() => void prepare()}>{t(actionKey)}</Button></AlertDialogTrigger> : null}
      <VmmNativeSessionRestart projectId={projectId} sessions={sessions} busy={busy || pending} onBusyChange={onBusyChange} onRunAction={onRunAction} onRefresh={onRefresh} />
    </div>
    {error ? <p role="alert" className="vk-field-error">{t("memory.sessionArchive.failed")}</p> : null}
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t(visiblePreview?.selection.archived ? "memory.sessionArchive.archive" : "memory.sessionArchive.unarchive")}</AlertDialogTitle>
          <AlertDialogDescription>{t("memory.sessionArchive.description")}</AlertDialogDescription>
        </AlertDialogHeader>
        <ul className="vk-mem-scroll-list vk-form-stack">
          {visiblePreview?.selection.sessionSourceIds.map((sessionSourceId) => {
            // Titles are presentation only; the accepted preview identity remains authoritative.
            // 标题只用于展示；已接纳预览中的身份始终具有权威性。
            const session = sessions.find((item) => item.sessionSourceId === sessionSourceId)
            return <li key={sessionSourceId}>
              <p className="vk-form-break vk-form-sm">{session?.displayTitle ?? sessionSourceId}</p>
              <p className="vk-form-codeline vk-faint">{sessionSourceId}</p>
            </li>
          })}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy || pending}>{t("memory.common.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={busy || pending || !visiblePreview} onClick={(event) => { event.preventDefault(); void confirm() }}>{t("memory.sessionArchive.confirm")}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
  </div></AlertDialog>
}
