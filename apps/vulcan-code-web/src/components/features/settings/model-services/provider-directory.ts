import { providerPriority } from "@/components/features/settings/provider-ordering"
import type {
  PublicAiProviderDescriptor,
  PublicAiProviderServiceTag,
  PublicAiSettingsSnapshot,
} from "@/types"

// ProviderTagFilter narrows the directory to one service family, or keeps every provider.
// ProviderTagFilter 将目录收窄到一个服务类别，或保留全部供应商。
export type ProviderTagFilter = "all" | PublicAiProviderServiceTag

// PROVIDER_SERVICE_TAGS lists the closed service families in display order.
// PROVIDER_SERVICE_TAGS 按显示顺序列出封闭的服务类别。
export const PROVIDER_SERVICE_TAGS: readonly PublicAiProviderServiceTag[] = ["model", "search", "analysis"]

// ProviderDirectory splits providers into the user's configured list and the recommended add list.
// ProviderDirectory 将供应商分为用户已配置列表与推荐可添加列表。
export interface ProviderDirectory {
  // configured providers in the user's own order (priority first).
  // 按用户自定顺序（优先级优先）排列的已配置供应商。
  configured: PublicAiProviderDescriptor[]
  // providers without any account, in the recommended directory order.
  // 尚无账号的供应商，按推荐目录顺序排列。
  available: PublicAiProviderDescriptor[]
}

// providerIsConfigured reports whether the user already set this provider up.
// providerIsConfigured 报告用户是否已配置该供应商。
// Parameters: snapshot supplies connections; provider is the exact descriptor. Returns true for custom
// providers and for providers with at least one account.
// 参数：snapshot 提供连接，provider 为精确描述。自定义供应商或至少有一个账号的供应商返回 true。
export function providerIsConfigured(
  snapshot: Pick<PublicAiSettingsSnapshot, "connections">,
  provider: Pick<PublicAiProviderDescriptor, "id" | "kind">,
): boolean {
  return provider.kind === "custom"
    || snapshot.connections.some((connection) => connection.provider_id === provider.id)
}

// directoryRank orders providers without a recommended rank after every ranked provider.
// directoryRank 让没有推荐名次的供应商排在所有有名次的供应商之后。
function directoryRank(provider: Pick<PublicAiProviderDescriptor, "directory_rank">): number {
  return provider.directory_rank ?? Number.MAX_SAFE_INTEGER
}

// buildProviderDirectory orders configured providers by user priority and the rest by recommendation.
// buildProviderDirectory 按用户优先级排列已配置供应商，其余按推荐顺序排列。
// Parameters: snapshot supplies providers, connections and sparse priorities. Returns both ordered lists;
// ties keep the registry order.
// 参数：snapshot 提供供应商、连接与稀疏优先级。返回两组有序列表；同级时保留注册表顺序。
export function buildProviderDirectory(
  snapshot: Pick<PublicAiSettingsSnapshot, "providers" | "connections" | "providerPriorities">,
): ProviderDirectory {
  const indexed = snapshot.providers.map((provider, index) => ({ provider, index }))
  const byRecommendation = (
    left: { provider: PublicAiProviderDescriptor; index: number },
    right: { provider: PublicAiProviderDescriptor; index: number },
  ) => directoryRank(left.provider) - directoryRank(right.provider) || left.index - right.index
  const configured = indexed
    .filter(({ provider }) => providerIsConfigured(snapshot, provider))
    .sort((left, right) => (
      providerPriority(snapshot.providerPriorities, right.provider.id)
      - providerPriority(snapshot.providerPriorities, left.provider.id)
      || byRecommendation(left, right)
    ))
    .map(({ provider }) => provider)
  const available = indexed
    .filter(({ provider }) => !providerIsConfigured(snapshot, provider))
    .sort(byRecommendation)
    .map(({ provider }) => provider)
  return { configured, available }
}

// providerMatchesFilter applies the tag chip and the free-text search to one provider.
// providerMatchesFilter 将标签筛选与文本搜索应用到一个供应商。
// Parameters: provider is the descriptor, localizedName its shown name, tag the chip, query the search text.
// 参数：provider 为描述，localizedName 为显示名称，tag 为标签筛选，query 为搜索文本。
export function providerMatchesFilter(
  provider: Pick<PublicAiProviderDescriptor, "id" | "display_name" | "service_tags">,
  localizedName: string,
  tag: ProviderTagFilter,
  query: string,
): boolean {
  if (tag !== "all" && !(provider.service_tags ?? []).includes(tag)) return false
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  return [provider.id, provider.display_name, localizedName].some((value) => value.toLowerCase().includes(needle))
}

// ProviderPriorityWrite is one priority value that must be saved to realize a new order.
// ProviderPriorityWrite 是实现新顺序所需保存的一个优先级值。
export interface ProviderPriorityWrite {
  providerId: string
  priority: number
}

// moveItem returns a copy of order with the item at from moved to index to.
// moveItem 返回将 from 处元素移到 to 位置后的副本。
export function moveItem<T>(order: readonly T[], from: number, to: number): T[] {
  const next = [...order]
  if (from < 0 || from >= next.length || to < 0 || to >= next.length || from === to) return next
  const [item] = next.splice(from, 1)
  next.splice(to, 0, item)
  return next
}

// priorityWritesForOrder converts a configured-provider order into the minimal priority writes.
// priorityWritesForOrder 将已配置供应商的顺序转换为最少的优先级写入。
// Parameters: order is the desired top-to-bottom provider id list; priorities are the persisted values.
// Returns the writes whose value differs from the current one; the first provider gets the largest value.
// 参数：order 为期望的自上而下供应商标识列表，priorities 为已持久化值。
// 返回值与当前值不同的写入；第一个供应商获得最大值。
export function priorityWritesForOrder(
  order: readonly string[],
  priorities: Readonly<Record<string, number>>,
): ProviderPriorityWrite[] {
  return order
    .map((providerId, index) => ({ providerId, priority: order.length - index }))
    .filter(({ providerId, priority }) => providerPriority(priorities, providerId) !== priority)
}
