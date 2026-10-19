import { AlertTriangle } from "lucide-react"
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/ui-kit/primitives/alert-dialog"
import { useI18n } from "@/i18n"
import type { VmmNativeRetentionPreview } from "@/types"

// Carries server-prepared impact and explicit callbacks; rendering never executes cleanup.
// 携带服务端准备的影响与明确回调，渲染绝不执行清洗。
interface CleanupPreviewProps {
  // Null closes the dialog; otherwise display this exact server preview.
  // 空值关闭对话框，否则显示服务端的这份准确预览。
  preview: VmmNativeRetentionPreview | null
  // Prevents duplicate submission while the action is pending.
  // 动作等待期间阻止重复提交。
  busy: boolean
  // Closes the preview without modifying memory.
  // 关闭预览，不修改记忆。
  onClose: () => void
  // Confirms the opaque preview identity through the parent action channel.
  // 通过父级动作通道确认不透明预览标识。
  onConfirm: () => void
  // Restores the initiating control when an asynchronous preview is opened outside a dialog trigger.
  // 在对话框触发器之外异步打开预览时，恢复发起控件的焦点。
  onRestoreFocus?: () => void
}

// Renders project names, counts and expandable identities before explicit confirmation.
// 在明确确认前渲染项目名称、计数及可展开标识。
// Returns a controlled dialog; callbacks run only for deliberate user actions.
// 返回受控对话框，仅在用户明确操作时运行回调。
export function VmmNativeCleanupPreview({ preview, busy, onClose, onConfirm, onRestoreFocus }: CleanupPreviewProps) {
  const { t } = useI18n()
  return <AlertDialog open={preview !== null} onOpenChange={(open) => { if (!open && !busy) onClose() }}>
    <AlertDialogContent className="flex max-h-[85dvh] min-h-0 flex-col overflow-hidden" size="md" onCloseAutoFocus={onRestoreFocus ? (event) => { event.preventDefault(); onRestoreFocus() } : undefined}>
      <AlertDialogHeader className="shrink-0">
        <AlertDialogTitle>{t("memory.maintenance.cleanupPreviewTitle")}</AlertDialogTitle>
        <AlertDialogDescription>{t("memory.maintenance.cleanupPreviewDescription")}</AlertDialogDescription>
      </AlertDialogHeader>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto">
        {preview?.projects.map((project) => <section className="rounded-lg border p-3" key={project.projectId}>
          <h3 className="text-sm font-medium">{project.projectName}</h3>
          <p className="mt-1 text-sm text-muted-foreground">{t("memory.maintenance.cleanupImpact")
            .replace("{memories}", String(project.recycledMemoryIds.length))
            .replace("{turns}", String(project.recycledTurnIds.length))
            .replace("{profiles}", String(project.expiredProfileNodeIds.length))
            .replace("{batches}", String(project.purgedBatchIds.length))}</p>
          <details className="mt-2 text-xs">
            <summary className="cursor-pointer">{t("memory.maintenance.cleanupRecordIds")}</summary>
            <dl className="mt-2 space-y-2 break-all font-mono">
              <dt>{t("memory.maintenance.memoryRecycleLimit")}</dt><dd>{project.recycledMemoryIds.join(", ") || "—"}</dd>
              <dt>{t("memory.maintenance.turns")}</dt><dd>{project.recycledTurnIds.join(", ") || "—"}</dd>
              <dt>{t("memory.maintenance.profileExpiryLimit")}</dt><dd>{project.expiredProfileNodeIds.join(", ") || "—"}</dd>
              <dt>{t("memory.maintenance.trashPurgeLimit")}</dt><dd>{project.purgedBatchIds.join(", ") || "—"}</dd>
            </dl>
          </details>
        </section>)}
      </div>
      <AlertDialogFooter className="shrink-0">
        <AlertDialogCancel disabled={busy}>{t("memory.common.cancel")}</AlertDialogCancel>
        <AlertDialogAction disabled={busy || preview === null} onClick={(event) => { event.preventDefault(); onConfirm() }}>
          <AlertTriangle className="mr-2 size-4" aria-hidden="true" />{t("memory.maintenance.runNow")}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
}
