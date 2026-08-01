// ModelEnabledFilter identifies the enabled-state subset shown by the model directory.
// ModelEnabledFilter 标识模型目录当前展示的启用状态子集。
export type ModelEnabledFilter = "all" | "enabled" | "disabled"

// FilterableProviderModel describes the exact searchable fields required by the model directory.
// FilterableProviderModel 描述模型目录筛选所需的精确可搜索字段。
export interface FilterableProviderModel {
  // Stable provider-scoped model identifier.
  // 供应商作用域内的稳定模型标识。
  modelId: string
  // User-visible model name.
  // 面向用户的模型名称。
  displayName: string
  // Effective model availability after applying persisted overrides.
  // 应用持久化覆盖后的模型有效可用状态。
  enabled: boolean
}

// filterProviderModels applies the model-name, model-id, and enabled-state filters without mutating input order.
// filterProviderModels 应用模型名称、模型标识与启用状态筛选，同时保持输入顺序不变。
//
// Parameters:
// 参数：
// models is the current provider model list, searchQuery is user text, and enabledFilter is the selected state subset.
// models 是当前供应商模型列表，searchQuery 是用户文本，enabledFilter 是选中的状态子集。
//
// Returns:
// 返回值：
// A filtered array preserving the provider catalog order.
// 返回保持供应商目录顺序的筛选数组。
export function filterProviderModels<T extends FilterableProviderModel>(
  models: readonly T[],
  searchQuery: string,
  enabledFilter: ModelEnabledFilter,
): T[] {
  const normalizedQuery = searchQuery.trim().toLocaleLowerCase()
  return models.filter((model) => {
    const matchesSearch = normalizedQuery.length === 0
      || model.displayName.toLocaleLowerCase().includes(normalizedQuery)
      || model.modelId.toLocaleLowerCase().includes(normalizedQuery)
    const matchesEnabledState = enabledFilter === "all"
      || (enabledFilter === "enabled" && model.enabled)
      || (enabledFilter === "disabled" && !model.enabled)
    return matchesSearch && matchesEnabledState
  })
}
