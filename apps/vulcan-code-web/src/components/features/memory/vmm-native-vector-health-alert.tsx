import * as React from "react"
import { AlertTriangle, RefreshCw } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { Button } from "@/ui-kit/primitives/button"
import { InlineDisclosure } from "@/ui-kit/primitives/disclosure"
import { useI18n } from "@/i18n"
import { nativeVectorRouteCause } from "./vmm-native-vector-status"
import type { VmmMemoryActionRequest, VmmMemoryActionResult, VmmNativeVectorStatusSummary } from "@/types"

// VmmNativeVectorHealthAlertProps supplies the read-only status endpoint and exact repair destinations.
// VmmNativeVectorHealthAlertProps 提供只读状态端点与准确的修复入口。
interface VmmNativeVectorHealthAlertProps {
  // The status request bypasses the page's exclusive write/action channel.
  // 状态请求绕开页面独占的写入和动作通道。
  onRunAction: (request: VmmMemoryActionRequest) => Promise<VmmMemoryActionResult>
  // Opens the guarded vector rebuild workflow in internal maintenance.
  // 打开内部维护中受保护的向量重建流程。
  onOpenRepair: () => void
  // Opens the embedding route settings when no current route can be resolved.
  // 无法解析当前路由时打开 Embedding 路由设置。
  onOpenEmbeddingSettings: () => void
}

// VmmNativeVectorHealthAlert exposes a global memory blocker where users browse or configure memory.
// VmmNativeVectorHealthAlert 在用户浏览或配置记忆的位置显示全局记忆阻断状态。
// Parameters supply the native status action and navigation callbacks; returns no markup when vectors are compatible.
// 参数提供原生状态动作与导航回调；向量兼容时不渲染警示。
export function VmmNativeVectorHealthAlert({
  onRunAction,
  onOpenRepair,
  onOpenEmbeddingSettings,
}: VmmNativeVectorHealthAlertProps) {
  const { t } = useI18n()
  // Status is read from the authoritative native vector generation check, not model connectivity.
  // 状态取自权威的原生向量代次检查，而非模型连通性。
  const [status, setStatus] = React.useState<VmmNativeVectorStatusSummary | null>(null)
  // A failed status read is visible instead of silently implying healthy memory.
  // 状态读取失败时显示错误，不能静默暗示记忆健康。
  const [error, setError] = React.useState<string | null>(null)
  const [loading, setLoading] = React.useState(true)
  // The generation prevents an old page or retry response from overwriting newer health.
  // 代次阻止旧页面或旧重试响应覆盖新的健康状态。
  const requestGeneration = React.useRef(0)
  const actionRef = React.useRef(onRunAction)
  actionRef.current = onRunAction

  // loadStatus rejects stale responses and keeps failed status checks visible and retryable.
  // loadStatus 丢弃过期响应，并让失败的状态检查保持可见且可重试。
  const loadStatus = React.useCallback(async () => {
    const generation = ++requestGeneration.current
    setLoading(true)
    setError(null)
    try {
      const result = await actionRef.current({
        action: "native_vector_status",
        commandId: crypto.randomUUID(),
        scope: "global",
        projectId: null,
        payload: {},
      })
      if (generation !== requestGeneration.current) return
      if (result.data.kind !== "native_vector_status") {
        setStatus(null)
        setError(t("memory.maintenance.invalidResponse"))
        return
      }
      setStatus(result.data.value)
    } catch (cause) {
      if (generation !== requestGeneration.current) return
      setStatus(null)
      setError(cause instanceof Error ? cause.message : String(cause))
    } finally {
      if (generation === requestGeneration.current) setLoading(false)
    }
  }, [t])

  React.useEffect(() => {
    // A discarded StrictMode mount must not start an obsolete status request.
    // 被丢弃的 StrictMode 挂载不得启动过期的状态请求。
    let active = true
    queueMicrotask(() => { if (active) void loadStatus() })
    return () => { active = false; requestGeneration.current += 1 }
  }, [loadStatus])

  if (loading && status === null && error === null) return null
  if (!error && status?.configured && status.rebuildRequiredMemoryCount === 0) return null

  // Active legacy vectors block all projects; recycled-only vectors block restoration only.
  // 活动旧向量阻断所有项目；只有回收站旧向量时仅阻断恢复操作。
  const activeBlocked = Boolean(status?.configured && status.activeRebuildRequiredMemoryCount > 0)
  const routeUnavailable = Boolean(status && !status.configured)
  const title = error
    ? t("memory.vectorHealth.checkFailed")
    : routeUnavailable
      ? t("memory.vectorHealth.routeUnavailable")
      : activeBlocked
        ? t("memory.vectorHealth.activeBlocked")
        : t("memory.vectorHealth.recycledBlocked")
  // The catalog cause is shown in the active language; the English diagnostic moves to details.
  // 目录原因以当前语言显示；英文诊断移入技术细节。
  const routeCause = status ? nativeVectorRouteCause(status) : null
  const routeDetail = status?.errorMessage && status.error ? status.error : null
  const description = error
    ? error
    : routeUnavailable
      ? routeCause || t("memory.vectorHealth.routeHelp")
      : activeBlocked
        ? t("memory.vectorHealth.activeHelp").replace("{count}", String(status?.activeRebuildRequiredMemoryCount))
        : t("memory.vectorHealth.recycledHelp").replace("{count}", String(status?.recycleBinRebuildRequiredMemoryCount))

  return (
    <Alert aria-live="polite" variant="destructive">
      <AlertTriangle aria-hidden="true" />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span>{description}</span>
          {routeUnavailable && routeDetail ? (
            <InlineDisclosure label={t("memory.vectorHealth.technicalDetails")}>
              <span className="break-all font-mono text-xs">{routeDetail}</span>
            </InlineDisclosure>
          ) : null}
        </div>
        {error ? (
          <Button disabled={loading} onClick={() => void loadStatus()} size="sm" type="button" variant="outline">
            <RefreshCw aria-hidden="true" className="size-4" />{t("memory.vectorHealth.retry")}
          </Button>
        ) : (
          <Button onClick={routeUnavailable ? onOpenEmbeddingSettings : onOpenRepair} size="sm" type="button" variant="outline">
            {t(routeUnavailable ? "memory.vectorHealth.openEmbeddingSettings" : "memory.vectorHealth.openRepair")}
          </Button>
        )}
      </AlertDescription>
    </Alert>
  )
}
