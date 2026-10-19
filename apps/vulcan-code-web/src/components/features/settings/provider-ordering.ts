import type { PublicAiProviderDescriptor } from "@/types"

// ProviderOrderingDescriptor is the exact provider subset needed by presentation ordering.
// ProviderOrderingDescriptor 是展示排序所需的精确供应商字段子集。
type ProviderOrderingDescriptor = Pick<
  PublicAiProviderDescriptor,
  "id" | "kind" | "service_entries"
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
