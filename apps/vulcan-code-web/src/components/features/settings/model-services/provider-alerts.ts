import { USAGE_CRITICAL_PERCENT, usageWindowPercentage } from "@/components/public-ai/credential-usage-overview"
import type { PublicAiAccountUsage, PublicAiSettingsSnapshot } from "@/types"

// ProviderAlertKind is one problem an account needs the user to look at.
// ProviderAlertKind 是账号需要用户关注的一种问题。
export type ProviderAlertKind = "catalog_error" | "pending_validation" | "quota_low"

// ProviderAlert names one account problem; the overview lists only these, never healthy accounts.
// ProviderAlert 描述一个账号问题；概览只列出这些问题，不列出正常账号。
export interface ProviderAlert {
  kind: ProviderAlertKind
  providerId: string
  connectionId: string
  connectionName: string
  // detail is the backend message for catalog errors.
  // detail 是目录错误的后端消息。
  detail?: string
}

// usageIsLow reports whether any available, limited quota window is nearly exhausted.
// usageIsLow 报告是否有可用且有限的额度窗口即将耗尽。
function usageIsLow(usage: PublicAiAccountUsage | undefined): boolean {
  return usage?.availability === "supported" && usage.quota_windows.some((window) => {
    if (window.unlimited) return false
    const percentage = usageWindowPercentage(window)
    return percentage !== null && percentage <= USAGE_CRITICAL_PERCENT
  })
}

// collectProviderAlerts derives the account problems from the snapshot and the latest usage reads.
// collectProviderAlerts 根据快照与最新用量读取推导账号问题。
// Parameters: snapshot is the authoritative state; usage holds observations newer than its cache.
// Returns alerts for enabled accounts only, in connection order.
// 参数：snapshot 为权威状态，usage 保存比其缓存更新的观测。仅返回已启用账号的问题，按连接顺序排列。
export function collectProviderAlerts(
  snapshot: PublicAiSettingsSnapshot,
  usage: Readonly<Record<string, PublicAiAccountUsage>>,
): ProviderAlert[] {
  const alerts: ProviderAlert[] = []
  for (const connection of snapshot.connections) {
    if (!connection.enabled) continue
    const base = { providerId: connection.provider_id, connectionId: connection.id, connectionName: connection.display_name }
    const provider = snapshot.providers.find((candidate) => candidate.id === connection.provider_id)
    const entry = provider?.service_entries.find((candidate) => candidate.id === connection.service_entry_id)
    // Provider-tool entries own no model catalog, so stale catalog errors do not apply to them.
    // 供应商工具入口不拥有模型目录，因此陈旧的目录错误不适用。
    const catalogError = entry?.kind === "provider_tools" ? undefined : snapshot.catalogErrors[connection.id]
    if (catalogError) alerts.push({ ...base, kind: "catalog_error", detail: catalogError })
    if (connection.pending_validation) alerts.push({ ...base, kind: "pending_validation" })
    if (usageIsLow(usage[connection.id] ?? snapshot.usageCache[connection.id])) alerts.push({ ...base, kind: "quota_low" })
  }
  return alerts
}

// HealthState summarizes an account or provider for status dots: failing, needing attention, healthy or switched off.
// HealthState 为状态圆点概括账号或供应商：失败、需关注、正常或已停用。
export type HealthState = "bad" | "warn" | "ok" | "off"

// alertSeverity ranks one alert kind; a failed catalog blocks the account, the others only need attention.
// alertSeverity 为一种提醒分级；目录失败会阻断账号，其余只需关注。
export function alertSeverity(kind: ProviderAlertKind): "bad" | "warn" {
  return kind === "catalog_error" ? "bad" : "warn"
}

// healthOf folds alerts into one state; enabled reports whether any covered account is switched on.
// healthOf 将提醒合并为一个状态；enabled 表示所覆盖的账号中是否有已启用的。
export function healthOf(alerts: readonly ProviderAlert[], enabled: boolean): HealthState {
  if (!enabled) return "off"
  if (alerts.some((alert) => alertSeverity(alert.kind) === "bad")) return "bad"
  return alerts.length > 0 ? "warn" : "ok"
}

// providerHealth maps every provider with accounts to its folded health.
// providerHealth 将每个有账号的供应商映射到合并后的健康状态。
export function providerHealth(snapshot: PublicAiSettingsSnapshot, alerts: readonly ProviderAlert[]): Record<string, HealthState> {
  const health: Record<string, HealthState> = {}
  for (const connection of snapshot.connections) {
    const providerId = connection.provider_id
    if (providerId in health) continue
    const enabled = snapshot.connections.some((candidate) => candidate.provider_id === providerId && candidate.enabled)
    health[providerId] = healthOf(alerts.filter((alert) => alert.providerId === providerId), enabled)
  }
  return health
}
