import type {
  PublicAiModel,
  PublicAiModelRouteRecord,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicModelProtocolRoute binds one discovered model to one configured service entry and protocol.
// PublicModelProtocolRoute 将一个发现模型绑定到一个已配置服务入口与协议。
export interface PublicModelProtocolRoute {
  // Number of credential accounts exposing this exact service-entry protocol route.
  // 暴露此精确服务入口协议路由的凭据账号数量。
  accountCount: number
  // Protocol variants available to every credential aggregated into this exact route.
  // 聚合到此精确路由中的每个凭据都可用的协议变体。
  availableProtocols: string[]
  // Stable backend binding-source identifier.
  // 稳定的后端绑定来源标识。
  bindingSource: NonNullable<PublicAiModelRouteRecord["bindingSource"]>
  // Stable manifest rule identifier when present.
  // 存在时的稳定 Manifest 规则标识。
  bindingRuleId?: string
  // Activated manifest revision owning this binding.
  // 持有此绑定的已激活 Manifest 修订。
  manifestRevision: number
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

// buildProviderModelGroups derives every manageable provider model independently from conversation publication.
// buildProviderModelGroups 独立于会话发布状态派生全部可管理供应商模型。
export function buildProviderModelGroups(
  snapshot: Pick<
    PublicAiSettingsSnapshot,
    "providers" | "connections" | "catalogs" | "modelSupportCatalogs" | "modelRouteRecords"
  >,
): PublicProviderModels[] {
  // connectionsById resolves only display/catalog ownership; protocol authority comes from
  // backend modelRouteRecords.
  // connectionsById 只解析显示与目录所有权；协议权威来自后端 modelRouteRecords。
  const connectionsById = new Map(snapshot.connections.map((connection) => [connection.id, connection]))
  return snapshot.providers
    .map((provider) => {
      const models = new Map<string, PublicModelProtocolRoute[]>()
      for (const record of snapshot.modelRouteRecords ?? []) {
        if (
          record.providerId !== provider.id
          || !record.executable
          || !record.protocolId
          || !record.bindingSource
        ) {
          continue
        }
        const connection = connectionsById.get(record.connectionId)
        if (
          !connection
          || connection.provider_id !== record.providerId
          || connection.service_entry_id !== record.serviceEntryId
        ) {
          continue
        }
        const effectiveCatalog = snapshot.catalogs[connection.id]
        const supportCatalog = snapshot.modelSupportCatalogs[connection.id] ?? effectiveCatalog
        const serviceEntry = provider.service_entries.find(
          (entry) => entry.id === record.serviceEntryId,
        )
        if (!effectiveCatalog || !supportCatalog || !serviceEntry) {
          continue
        }
        const model = supportCatalog.models.find((candidate) => candidate.id === record.modelId)
        if (!model) {
          continue
        }
        const accountEnabled = effectiveCatalog.models.some((candidate) => candidate.id === record.modelId)
        const routes = models.get(model.id) ?? []
        // availableProtocols is a canonical set so route aggregation never depends on DTO order.
        // availableProtocols 是规范集合，确保路由聚合绝不依赖 DTO 顺序。
        const availableProtocols = [...new Set(record.availableProtocols)].sort()
        const existing = routes.find((route) => (
          route.serviceEntryId === record.serviceEntryId
          && route.protocolId === record.protocolId
          && route.bindingSource === record.bindingSource
          && route.bindingRuleId === record.bindingRuleId
          && route.manifestRevision === record.manifestRevision
          && route.availableProtocols.length === availableProtocols.length
          && route.availableProtocols.every((protocolId, index) => (
            protocolId === availableProtocols[index]
          ))
        ))
        if (existing) {
          existing.accountCount += Number(accountEnabled)
        } else {
          routes.push({
            accountCount: Number(accountEnabled),
            availableProtocols,
            bindingSource: record.bindingSource,
            bindingRuleId: record.bindingRuleId,
            manifestRevision: record.manifestRevision,
            model,
            protocolId: record.protocolId,
            serviceEntryId: serviceEntry.id,
            serviceEntryName: serviceEntry.display_name,
          })
        }
        models.set(model.id, routes)
      }
      return { provider, models }
    })
    .filter((group) => group.models.size > 0)
}
