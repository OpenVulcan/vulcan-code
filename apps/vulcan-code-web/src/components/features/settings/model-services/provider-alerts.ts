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
