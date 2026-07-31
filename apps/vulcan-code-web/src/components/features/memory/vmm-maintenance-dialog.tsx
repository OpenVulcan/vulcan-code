import * as React from "react"
import { CheckCircle2, DatabaseBackup, OctagonX, TriangleAlert } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Progress } from "@/components/ui/progress"
import { useI18n } from "@/i18n"
import type { VmmMaintenanceSummary } from "@/types"

// VmmMaintenanceDialogProps defines the durable maintenance workflow and runtime restart.
// VmmMaintenanceDialogProps 定义长期维护流程与运行时重启。
interface VmmMaintenanceDialogProps {
  busy: boolean
  maintenance: VmmMaintenanceSummary
  onRun: (action: "managed_maintenance_start" | "managed_maintenance_status" | "managed_maintenance_cancel") => Promise<VmmMaintenanceSummary | null>
  onStartRuntime: () => Promise<boolean>
}

// VmmMaintenanceDialog renders guarded backup, rebuild, cancellation, and recovery.
// VmmMaintenanceDialog 渲染受保护的备份、重建、取消与恢复流程。
function VmmMaintenanceDialog({
  busy,
  maintenance: authoritativeMaintenance,
  onRun,
  onStartRuntime,
}: VmmMaintenanceDialogProps) {
  const { t } = useI18n()
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [progressOpen, setProgressOpen] = React.useState(false)
  const [maintenance, setMaintenance] = React.useState(authoritativeMaintenance)
  const [runtimeStarting, setRuntimeStarting] = React.useState(false)
  const [runtimeStartFailed, setRuntimeStartFailed] = React.useState(false)

  React.useEffect(() => {
    setMaintenance(authoritativeMaintenance)
    if (isActiveMaintenance(authoritativeMaintenance)) {
      setProgressOpen(true)
    }
  }, [authoritativeMaintenance])

  React.useEffect(() => {
    if (!progressOpen || !isActiveMaintenance(maintenance)) {
      return
    }
    let disposed = false
    const timer = window.setTimeout(() => {
      void onRun("managed_maintenance_status").then((next) => {
        if (!disposed && next) {
          setMaintenance(next)
        }
      })
    }, 750)
    return () => {
      disposed = true
      window.clearTimeout(timer)
    }
  }, [maintenance, onRun, progressOpen])

  // startMaintenance accepts the guarded confirmation and opens persistent progress.
  // startMaintenance 接受受保护确认并打开持久进度。
  const startMaintenance = React.useCallback(async () => {
    const next = await onRun("managed_maintenance_start")
    if (!next) {
      return
    }
    setConfirmOpen(false)
    setProgressOpen(true)
    setMaintenance(next)
  }, [onRun])

  // cancelMaintenance requests cooperative cancellation without deleting original data.
  // cancelMaintenance 请求协作式取消且不删除原始数据。
  const cancelMaintenance = React.useCallback(async () => {
    const next = await onRun("managed_maintenance_cancel")
    if (next) {
      setMaintenance(next)
    }
  }, [onRun])

  // startMemoryRuntime restarts the managed service and closes progress only after success.
  // startMemoryRuntime 重启托管服务，并仅在成功后关闭进度弹窗。
  const startMemoryRuntime = React.useCallback(async () => {
    setRuntimeStarting(true)
    setRuntimeStartFailed(false)
    try {
      const started = await onStartRuntime()
      if (started) {
        setProgressOpen(false)
      } else {
        setRuntimeStartFailed(true)
      }
    } finally {
      setRuntimeStarting(false)
    }
  }, [onStartRuntime])

  return (
    <>
      <Button disabled={busy} onClick={() => setConfirmOpen(true)} size="sm" type="button">
        <DatabaseBackup className="size-4" aria-hidden="true" />
        {t("memory.advanced.backupRebuild")}
      </Button>

      <AlertDialog onOpenChange={setConfirmOpen} open={confirmOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("memory.maintenance.confirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("memory.maintenance.confirmDescription")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <Alert variant="default">
            <DatabaseBackup aria-hidden="true" />
            <AlertTitle>{t("memory.maintenance.backupFirst")}</AlertTitle>
            <AlertDescription>{t("memory.maintenance.backupFirstDescription")}</AlertDescription>
          </Alert>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>
              {t("memory.maintenance.later")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                event.preventDefault()
                void startMaintenance()
              }}
            >
              {t("memory.advanced.backupRebuild")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog
        onOpenChange={(open) => {
          if (!isActiveMaintenance(maintenance)) {
            setProgressOpen(open)
          }
        }}
        open={progressOpen}
      >
        <DialogContent
          className="sm:max-w-lg"
          onEscapeKeyDown={(event) => {
            if (isActiveMaintenance(maintenance)) {
              event.preventDefault()
            }
          }}
          onInteractOutside={(event) => {
            if (isActiveMaintenance(maintenance)) {
              event.preventDefault()
            }
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("memory.maintenance.progressTitle")}</DialogTitle>
            <DialogDescription>{t("memory.maintenance.progressDescription")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <MaintenanceOutcome maintenance={maintenance} />
            {runtimeStartFailed ? (
              <Alert variant="destructive">
                <TriangleAlert aria-hidden="true" />
                <AlertTitle>{t("memory.maintenance.startFailed")}</AlertTitle>
                <AlertDescription>{t("memory.maintenance.startFailedDescription")}</AlertDescription>
              </Alert>
            ) : null}
            <div className="space-y-2">
              <div className="flex items-center justify-between text-sm">
                <span>{maintenance.message || t("memory.common.loading")}</span>
                <span className="tabular-nums">{maintenance.progressPercent}%</span>
              </div>
              <Progress value={maintenance.progressPercent} />
            </div>
          </div>
          <DialogFooter>
            {maintenance.canCancel ? (
              <Button
                disabled={busy}
                onClick={() => void cancelMaintenance()}
                type="button"
                variant="outline"
              >
                {t("memory.maintenance.cancel")}
              </Button>
            ) : null}
            {maintenance.phase === "completed" ? (
              <Button
                disabled={busy || runtimeStarting}
                onClick={() => void startMemoryRuntime()}
                type="button"
              >
                {runtimeStarting
                  ? t("memory.common.loading")
                  : t("memory.maintenance.startMemory")}
              </Button>
            ) : null}
            {!isActiveMaintenance(maintenance) && maintenance.phase !== "completed" ? (
              <Button onClick={() => setProgressOpen(false)} type="button" variant="outline">
                {t("memory.common.close")}
              </Button>
            ) : null}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

// MaintenanceOutcome renders a compact terminal or in-progress state.
// MaintenanceOutcome 渲染紧凑的终态或进行中状态。
function MaintenanceOutcome({ maintenance }: { maintenance: VmmMaintenanceSummary }) {
  const { t } = useI18n()
  if (maintenance.phase === "completed") {
    return (
      <Alert>
        <CheckCircle2 aria-hidden="true" />
        <AlertTitle>{t("memory.maintenance.completed")}</AlertTitle>
        <AlertDescription>{maintenance.message}</AlertDescription>
      </Alert>
    )
  }
  if (maintenance.phase === "failed") {
    return (
      <Alert variant="destructive">
        <TriangleAlert aria-hidden="true" />
        <AlertTitle>{t("memory.maintenance.failed")}</AlertTitle>
        <AlertDescription>{maintenance.message}</AlertDescription>
      </Alert>
    )
  }
  if (maintenance.phase === "cancelled") {
    return (
      <Alert>
        <OctagonX aria-hidden="true" />
        <AlertTitle>{t("memory.maintenance.cancelled")}</AlertTitle>
        <AlertDescription>{maintenance.message}</AlertDescription>
      </Alert>
    )
  }
  return null
}

// isActiveMaintenance identifies phases that must retain the progress dialog.
// isActiveMaintenance 标识必须保留进度弹窗的阶段。
function isActiveMaintenance(maintenance: VmmMaintenanceSummary): boolean {
  return ["backing_up", "rebuilding", "cancelling"].includes(maintenance.phase)
}

export { VmmMaintenanceDialog }
export type { VmmMaintenanceDialogProps }
