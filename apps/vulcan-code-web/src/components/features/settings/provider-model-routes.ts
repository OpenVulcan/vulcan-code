import type {
  PublicAiModel,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicModelProtocolRoute binds one discovered model to one configured service entry and protocol.
// PublicModelProtocolRoute 将一个发现模型绑定到一个已配置服务入口与协议。
export interface PublicModelProtocolRoute {
  // Number of credential accounts exposing this exact service-entry protocol route.
  // 暴露此精确服务入口协议路由的凭据账号数量。
  accountCount: number
  // Credential-discovered model snapshot used for exact capability controls.
  // 用于精确能力控件的凭据发现模型快照。
  model: PublicAiModel
  // Exact protocol profile identifier.
  // 精确协议配置档标识。
  protocolId: string
  // Exact configured service-entry identifier.
  // 精确已配置服务入口标识。
  serviceEntryId: string
  // User-facing name of the exact configured service entry.
  // 精确已配置服务入口的用户可见名称。
  serviceEntryName: string
}

// PublicProviderModels groups deduplicated models under one registered provider.
// PublicProviderModels 将去重模型归组到一个已注册供应商下。
export interface PublicProviderModels {
  // Registered provider descriptor.
  // 已注册供应商描述。
  provider: PublicAiProviderDescriptor
  // Service-entry and protocol scoped model routes keyed by exact model identifier.
  // 按精确模型标识索引的服务入口与协议作用域模型路由。
  models: Map<string, PublicModelProtocolRoute[]>
}

// compactServiceEntryName shortens common geographic and account-scope words for route tabs.
// compactServiceEntryName 为路由页签缩短常见地域与账号范围词。
export function compactServiceEntryName(displayName: string): string {
  return displayName
    .replace(/\bPersonal\b/gi, "")
    .replace(/\bInternational\b/gi, "Intl")
    .replace(/\bChina\b/gi, "CN")
    .replace(/\bSingapore\b/gi, "SG")
    .replace(/\s+/g, " ")
    .trim()
}

// buildProviderModelGroups derives stable provider routes from support catalogs and counts enabled accounts from effective catalogs.
// buildProviderModelGroups 从支持目录派生稳定供应商路由，并从有效目录统计已启用账号。
export function buildProviderModelGroups(
  snapshot: Pick<
    PublicAiSettingsSnapshot,
    "providers" | "connections" | "catalogs" | "modelSupportCatalogs"
  >,
): PublicProviderModels[] {
  return snapshot.providers
    .map((provider) => {
      const models = new Map<string, PublicModelProtocolRoute[]>()
      for (const connection of snapshot.connections) {
        if (connection.provider_id !== provider.id) {
          continue
        }
        const effectiveCatalog = snapshot.catalogs[connection.id]
        const supportCatalog = snapshot.modelSupportCatalogs[connection.id] ?? effectiveCatalog
        const serviceEntry = provider.service_entries.find(
          (entry) => entry.id === connection.service_entry_id,
        )
        if (!supportCatalog || !serviceEntry) {
          continue
        }
        const enabledModelIds = new Set(effectiveCatalog?.models.map((model) => model.id) ?? [])
        for (const model of supportCatalog.models) {
          const routes = models.get(model.id) ?? []
          const existing = routes.find((route) => (
            route.serviceEntryId === connection.service_entry_id
            && route.protocolId === connection.selected_protocol
          ))
          if (existing) {
            if (enabledModelIds.has(model.id)) {
              existing.accountCount += 1
            }
          } else {
            routes.push({
              accountCount: enabledModelIds.has(model.id) ? 1 : 0,
              model,
              protocolId: connection.selected_protocol,
              serviceEntryId: serviceEntry.id,
              serviceEntryName: serviceEntry.display_name,
            })
          }
          models.set(model.id, routes)
        }
      }
      return { provider, models }
    })
    .filter((group) => group.models.size > 0)
}
