import type { PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"

// ProviderOrderingDescriptor is the exact provider subset needed by presentation ordering.
// ProviderOrderingDescriptor 是展示排序所需的精确供应商字段子集。
type ProviderOrderingDescriptor = Pick<
  PublicAiProviderDescriptor,
  "id" | "kind" | "promotion" | "service_entries"
>

// providerPriority resolves the sparse user value and applies the zero default.
// providerPriority 解析稀疏用户值并应用零默认值。
//
// Parameters: priorities contains persisted sparse values and providerId selects one provider.
// 参数：priorities 包含持久化稀疏值，providerId 选择一个供应商。
// Returns: the explicit priority or zero when no override exists.
// 返回值：返回显式优先级；没有覆盖时返回零。
export function providerPriority(
  priorities: Readonly<Record<string, number>>,
  providerId: string,
): number {
  return priorities[providerId] ?? 0
}

// isFunctionalProvider identifies system providers whose entries expose provider tools only.
// isFunctionalProvider 识别仅公开供应商工具入口的系统供应商。
//
// Parameters: provider is the exact registered descriptor being classified.
// 参数：provider 是正在分类的精确注册描述。
// Returns: true only for an immutable provider whose every service entry is provider-tools.
// 返回值：仅当不可变供应商的全部服务入口都是供应商工具时返回 true。
export function isFunctionalProvider(provider: ProviderOrderingDescriptor): boolean {
  return provider.kind === "system"
    && provider.service_entries.length > 0
    && provider.service_entries.every((entry) => entry.kind === "provider_tools")
}

// providerHasConfiguredModels reports whether the current executable catalog contains one model route.
// providerHasConfiguredModels 报告当前可执行目录是否包含一条模型路由。
//
// Parameters: snapshot supplies executable route records and providerId selects their owner.
// 参数：snapshot 提供可执行路由记录，providerId 选择其所有者。
// Returns: true when at least one executable model route belongs to the provider.
// 返回值：当至少一条可执行模型路由属于该供应商时返回 true。
export function providerHasConfiguredModels(
  snapshot: Pick<PublicAiSettingsSnapshot, "modelRouteRecords">,
  providerId: string,
): boolean {
  return snapshot.modelRouteRecords?.some((record) => (
    record.providerId === providerId && record.executable
  )) ?? false
}

// orderProvidersByPriority sorts high priorities first while retaining promotional and registry tie-breaks.
// orderProvidersByPriority 将高优先级排在前面，并保留推广与注册表的同级排序。
//
// Parameters: providers is the authoritative registry order and priorities contains sparse overrides.
// 参数：providers 是权威注册顺序，priorities 包含稀疏覆盖。
// Returns: a new high-to-low ordered array without mutating the input.
// 返回值：返回新的从高到低排序数组，不修改输入。
export function orderProvidersByPriority<T extends Pick<PublicAiProviderDescriptor, "id" | "promotion">>(
  providers: readonly T[],
  priorities: Readonly<Record<string, number>>,
): T[] {
  // indexedProviders preserves the authoritative registry position as the final stable tie-break.
  // indexedProviders 保留权威注册表位置，作为最终稳定同级排序依据。
  const indexedProviders = providers.map((provider, index) => ({ provider, index }))
  return indexedProviders
    .sort((left, right) => (
      providerPriority(priorities, right.provider.id)
      - providerPriority(priorities, left.provider.id)
      || Number(Boolean(left.provider.promotion)) - Number(Boolean(right.provider.promotion))
      || left.index - right.index
    ))
    .map(({ provider }) => provider)
}

// orderCredentialDirectoryProviders groups providers with configured model routes before priority ordering.
// orderCredentialDirectoryProviders 先按已配置模型路由分组，再按优先级排列凭据目录供应商。
//
// Parameters: snapshot provides every provider, sparse priority, and executable model route.
// 参数：snapshot 提供全部供应商、稀疏优先级与可执行模型路由。
// Returns: a new array ordered by configured-model status, priority, then registry position.
// 返回值：返回按模型配置状态、优先级、注册位置排列的新数组。
export function orderCredentialDirectoryProviders(
  snapshot: Pick<
    PublicAiSettingsSnapshot,
    "providers" | "providerPriorities" | "modelRouteRecords"
  >,
): PublicAiProviderDescriptor[] {
  // indexedProviders preserves registry order after the two requested ordering levels.
  // indexedProviders 在两个指定排序层级之后保留注册表顺序。
  const indexedProviders = snapshot.providers.map((provider, index) => ({
    provider,
    index,
    hasConfiguredModels: providerHasConfiguredModels(snapshot, provider.id),
  }))
  return indexedProviders
    .sort((left, right) => (
      Number(right.hasConfiguredModels) - Number(left.hasConfiguredModels)
      || providerPriority(snapshot.providerPriorities, right.provider.id)
        - providerPriority(snapshot.providerPriorities, left.provider.id)
      || left.index - right.index
    ))
    .map(({ provider }) => provider)
}
