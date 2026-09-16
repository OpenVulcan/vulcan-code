import type {
  PublicAiModel,
  PublicAiModelRouteRecord,
  PublicAiProviderDescriptor,
  PublicAiProviderToolCapabilityKind,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicModelProtocolRoute binds one discovered model to one configured service entry and protocol.
// PublicModelProtocolRoute 将一个发现模型绑定到一个已配置服务入口与协议。
export interface PublicModelProtocolRoute {
  // Number of credential accounts exposing this exact service-entry protocol route.
  // 暴露此精确服务入口协议路由的凭据账号数量。
  accountCount: number
  // Exact credential identities and admission states sharing this resolved route.
  // 共享此解析路由的精确凭据身份与准入状态。
  accounts: Array<{ connectionId: string; displayName: string; enabled: boolean }>
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

// PublicProviderModelInventoryEntry keeps every catalog or signed extension model visible while protocol routes remain exact.
// PublicProviderModelInventoryEntry 保留所有目录及签名扩展模型，同时让协议路由保持精确。
export interface PublicProviderModelInventoryEntry {
  // User-facing name and exact provider-scoped identifier.
  // 面向用户的名称和精确供应商作用域标识。
  modelId: string
  displayName: string
  // Normalized catalog models retain editable capability contracts across account routes.
  // 归一化目录模型跨账号路由保留可编辑能力契约。
  catalogModels: PublicAiModel[]
  // Only backend-resolved conversation protocol routes may open protocol settings.
  // 只有后端已解析的会话协议路由可以打开协议设置。
  protocolRoutes: PublicModelProtocolRoute[]
  // Signed provider-extension capabilities remain distinct from internal inference operations.
  // 签名供应商扩展能力与内部推理操作保持独立。
  extensionCapabilities: PublicAiProviderToolCapabilityKind[]
  // Available account count is deduplicated by exact connection identity.
  // 可用账号数按精确连接身份去重。
  availableAccountCount: number
}

// PublicProviderModelInventory groups the complete account and signed-template inventory by provider.
// PublicProviderModelInventory 按供应商归组完整账号目录与签名模板目录。
export interface PublicProviderModelInventory {
  // Registered provider owns each model identity.
  // 已注册供应商拥有每个模型身份。
  provider: PublicAiProviderDescriptor
  // One row per exact upstream model identifier.
  // 每个精确上游模型标识对应一行。
  models: Map<string, PublicProviderModelInventoryEntry>
}

// buildProviderModelInventoryGroups combines full support catalogs and signed template models without inventing protocol bindings.
// buildProviderModelInventoryGroups 合并完整支持目录与签名模板模型，不推测协议绑定。
// Parameters: snapshot is one coherent settings generation; returns manageable provider groups.
// 参数：snapshot 是同一代际的设置快照；返回可管理的供应商分组。
export function buildProviderModelInventoryGroups(
  snapshot: Pick<PublicAiSettingsSnapshot,
    "providers" | "connections" | "catalogs" | "modelSupportCatalogs" | "modelRouteRecords"
    | "providerToolTemplateModels" | "providerToolTargets">,
): PublicProviderModelInventory[] {
  // Protocol routes are already resolved by the backend and may be absent for native operations.
  // 协议路由已经由后端解析，原生操作可以没有协议路由。
  const routesByProvider = new Map(buildProviderModelGroups(snapshot).map((group) => [group.provider.id, group.models]))
  // connectionsById supplies the registered provider owner for each account catalog.
  // connectionsById 为每份账号目录提供已注册供应商归属。
  const connectionsById = new Map(snapshot.connections.map((connection) => [connection.id, connection]))
  return snapshot.providers.map((provider) => {
    // Entries collect source-authoritative models before account availability is aggregated.
    // 条目先收集来源权威模型，再汇总账号可用性。
    const entries = new Map<string, PublicProviderModelInventoryEntry>()
    // availableAccounts prevents duplicate counts when catalog and extension expose one account.
    // availableAccounts 避免目录与扩展共用账号时重复计数。
    const availableAccounts = new Map<string, Set<string>>()
    // ensureEntry creates only an inventory row, never a fabricated protocol route.
    // ensureEntry 只建立目录行，绝不伪造协议路由。
    // Parameters: id and name identify one model; returns its shared row.
    // 参数：id 与 name 标识一个模型；返回其共享行。
    const ensureEntry = (modelId: string, displayName: string): PublicProviderModelInventoryEntry => {
      // existing preserves the first authoritative display name for a provider-scoped model.
      // existing 保留供应商作用域模型最先取得的权威展示名称。
      const existing = entries.get(modelId)
      if (existing) return existing
      // entry starts with exact protocol routes or an empty native-only route set.
      // entry 以精确协议路由或空的纯原生路由集建立。
      const entry: PublicProviderModelInventoryEntry = {
        modelId,
        displayName,
        catalogModels: [],
        protocolRoutes: routesByProvider.get(provider.id)?.get(modelId) ?? [],
        extensionCapabilities: [],
        availableAccountCount: 0,
      }
      entries.set(modelId, entry)
      return entry
    }
    for (const [connectionId, catalog] of Object.entries(snapshot.modelSupportCatalogs)) {
      // connection identifies the provider that owns this exact support catalog.
      // connection 标识这份精确支持目录所属的供应商。
      const connection = connectionsById.get(connectionId)
      if (connection?.provider_id !== provider.id) continue
      // effectiveModels distinguishes configured availability from support-only visibility.
      // effectiveModels 区分已配置可用状态和仅支持目录可见状态。
      const effectiveModels = new Set(snapshot.catalogs[connectionId]?.models.map((model) => model.id) ?? [])
      for (const model of catalog.models) {
        // entry shares one row across this provider's account catalogs.
        // entry 在此供应商各账号目录间共享一行。
        const entry = ensureEntry(model.id, model.display_name)
        entry.catalogModels.push(model)
        if (effectiveModels.has(model.id)) {
          // accounts records this exact available credential only once.
          // accounts 仅记录一次此精确可用凭据。
          const accounts = availableAccounts.get(model.id) ?? new Set<string>()
          accounts.add(connectionId)
          availableAccounts.set(model.id, accounts)
        }
      }
    }
    for (const template of snapshot.providerToolTemplateModels) {
      if (template.providerId !== provider.id || !connectionsById.has(template.connectionId)) continue
      // entry merges signed operations with an existing catalog identity when present.
      // entry 在存在目录身份时将签名操作合并到已有模型行。
      const entry = ensureEntry(template.modelId, template.displayName)
      // Current development hosts may still serve the previous template DTO; exact available targets supply its capabilities.
      // 当前开发宿主可能仍返回旧版模板 DTO；用精确可用目标补充其能力。
      const templateCapabilities = template.capabilities ?? snapshot.providerToolTargets.filter((target) => (
        target.connectionId === template.connectionId
        && target.target.provider_id === provider.id
        && target.target.upstream_target_id === template.modelId
      )).map((target) => target.target.contract.capability)
      for (const capability of templateCapabilities) {
        if (!entry.extensionCapabilities.includes(capability)) entry.extensionCapabilities.push(capability)
      }
      if (snapshot.providerToolTargets.some((target) => (
        target.connectionId === template.connectionId
        && target.target.provider_id === provider.id
        && target.target.upstream_target_id === template.modelId
        && templateCapabilities.includes(target.target.contract.capability)
      ))) {
        // accounts joins extension availability with native catalog availability.
        // accounts 将扩展可用性与原生目录可用性合并。
        const accounts = availableAccounts.get(template.modelId) ?? new Set<string>()
        accounts.add(template.connectionId)
        availableAccounts.set(template.modelId, accounts)
      }
    }
    for (const entry of entries.values()) {
      entry.availableAccountCount = availableAccounts.get(entry.modelId)?.size ?? 0
    }
    return { provider, models: entries }
  }).filter((group) => group.models.size > 0)
}

// filterProviderModelInventoryGroupsWithAvailableAccounts limits the global list to models enabled by at least one account.
// filterProviderModelInventoryGroupsWithAvailableAccounts 将全局列表限定为至少被一个账号启用的模型。
// Parameters: groups is the complete management inventory; returns display-only groups without changing account catalogs.
// 参数：groups 是完整的管理目录；返回仅用于显示的分组，不改变账号目录。
export function filterProviderModelInventoryGroupsWithAvailableAccounts(
  groups: PublicProviderModelInventory[],
): PublicProviderModelInventory[] {
  return groups.map((group) => ({
    ...group,
    models: new Map(Array.from(group.models).filter(([, model]) => model.availableAccountCount > 0)),
  })).filter((group) => group.models.size > 0)
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
// Parameters: snapshot supplies provider descriptors, exact account catalogs and resolved route records; returns provider/model groups retaining service, protocol and account admission identity.
// 参数：snapshot 提供供应商描述、精确账号目录及已解析路由记录；返回保留服务、协议及账号准入身份的供应商模型分组。
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
          existing.accounts.push({ connectionId: connection.id, displayName: connection.display_name, enabled: accountEnabled })
        } else {
          routes.push({
            accountCount: Number(accountEnabled),
            accounts: [{ connectionId: connection.id, displayName: connection.display_name, enabled: accountEnabled }],
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
