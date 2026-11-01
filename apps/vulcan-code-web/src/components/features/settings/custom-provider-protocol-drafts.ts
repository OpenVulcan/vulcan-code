import type {
  PublicAiCustomProviderProtocol,
  PublicAiCustomProviderProtocolRoute,
  PublicAiCustomModelProtocolPreference,
  PublicAiModelRouteRecord,
  PublicAiResponsesWebsocketPolicy,
  PublicAiServiceEntry,
} from "@/types"

// CustomProtocolRouteDraft holds the browser-editable per-protocol route overrides for one
// enabled protocol. Empty values inherit the shared provider projection on save (plan §R6.5).
// CustomProtocolRouteDraft 为一个启用协议保存浏览器可编辑的逐协议路由覆盖。保存时空值
// 继承共享供应商投影（方案 §R6.5）。
export interface CustomProtocolRouteDraft {
  // Route-specific base URL override; empty inherits the shared provider base URL.
  // 路由专属基础 URL 覆盖；空值继承共享供应商基础 URL。
  baseUrlOverride: string
  // Route-specific endpoint path; empty uses the protocol default endpoint.
  // 路由专属端点路径；空值使用协议默认端点。
  endpointPath: string
  // Route-specific API-key projection; empty inherits the shared provider projection.
  // 路由专属 API Key 投影；空值继承共享供应商投影。
  authentication: "bearer_authorization" | "google_api_key" | "x_api_key" | ""
  // Bounded non-sensitive fixed request headers for this route only.
  // 仅作用于该路由的有界非敏感固定请求头。
  requestHeaders: Record<string, string>
}

// CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE is the non-empty UI sentinel required by
// Radix Select for a route field that persists as an empty inherited override.
// CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE 是 Radix Select 要求的非空 UI 哨兵，
// 对应持久化为空值的路由继承覆盖。
export const CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE = "__vulcan_route_inherit__"

/**
 * Projects one persisted route override into the non-empty value required by the Select UI.
 * 把一个持久化路由覆盖投影为 Select UI 要求的非空值。
 *
 * @param value Persisted route override; an empty string means inherit.
 * @param value 持久化路由覆盖；空字符串表示继承。
 * @returns The exact override or the non-empty inherit sentinel.
 * @returns 精确覆盖值或非空继承哨兵。
 */
export function customProtocolRouteSelectValue(value: string): string {
  return value === "" ? CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE : value
}

/**
 * Converts one Select UI value back into the persisted route-override representation.
 * 把一个 Select UI 值转换回持久化路由覆盖表示。
 *
 * @param value Non-empty Select value selected by the user.
 * @param value 用户选择的非空 Select 值。
 * @returns An empty string for inheritance, otherwise the exact selected override.
 * @returns 继承时返回空字符串，否则返回精确选定覆盖值。
 */
export function customProtocolRouteDraftValue(value: string): string {
  return value === CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE ? "" : value
}

// CodexResponsesFeaturePolicies is the single browser-authoritative capability state for the
// unique Codex Responses route in one custom provider.
// CodexResponsesFeaturePolicies 是一个自定义供应商中唯一 Codex Responses 路由的
// 浏览器端单一权威能力状态。
export interface CodexResponsesFeaturePolicies {
  // Explicit WebSocket policy saved onto the exact Codex Responses route.
  // 保存到精确 Codex Responses 路由的显式 WebSocket 策略。
  websocketPolicy: PublicAiResponsesWebsocketPolicy
}

// CustomProviderProtocolCollection is the complete browser-editable protocol set plus one
// route draft per enabled protocol.
// CustomProviderProtocolCollection 是完整可编辑协议集合，外加每个启用协议一份路由草稿。
export interface CustomProviderProtocolCollection {
  // Enabled protocols in fixed Auto-priority order; never empty.
  // 按固定 Auto 优先级排序的启用协议；绝不为空。
  protocols: PublicAiCustomProviderProtocol[]
  // One route draft per enabled protocol, keyed by the browser protocol identifier.
  // 每个启用协议一份路由草稿，按浏览器协议标识索引。
  drafts: Partial<Record<PublicAiCustomProviderProtocol, CustomProtocolRouteDraft>>
}

/**
 * Returns the backend-resolved route for one exact provider, model, and credential connection.
 * 返回一个精确供应商、模型与凭据连接对应的后端已解析路由。
 *
 * @param records Backend-authoritative model route records from the settings snapshot.
 * @param records 设置快照中的后端权威模型路由记录。
 * @param providerId Exact provider identifier.
 * @param providerId 精确供应商标识。
 * @param modelId Exact upstream model identifier.
 * @param modelId 精确上游模型标识。
 * @param connectionId Exact selected credential connection; missing identity cannot fall back.
 * @param connectionId 精确选定凭据连接；缺失身份时不得回退。
 * @returns The exact backend status record, including a blocked route; undefined means no account record exists.
 * @returns 精确后端状态记录（包括被阻止路由）；undefined 表示不存在该账号记录。
 */
export function exactCustomModelRouteRecord(
  records: PublicAiModelRouteRecord[] | undefined,
  providerId: string,
  modelId: string,
  connectionId: string | null | undefined,
): PublicAiModelRouteRecord | undefined {
  if (!connectionId) {
    return undefined
  }
  return records?.find((record) => (
    record.providerId === providerId
    && record.modelId === modelId
    && record.connectionId === connectionId
  ))
}

/**
 * Intersects provider-enabled protocols with backend-observed variants for explicit selection.
 * 将供应商已启用协议与后端观测变体取交集，作为显式选择集合。
 *
 * @param enabledProtocols Provider-level protocols declared by the activated custom descriptor.
 * @param enabledProtocols 已激活自定义描述符声明的供应商级协议。
 * @param routeRecord Exact backend model/account route status when an account catalog exists.
 * @param routeRecord 存在账号目录时的精确后端模型/账号路由状态。
 * @returns Protocols the current model/account can explicitly select, preserving provider order.
 * @returns 当前模型/账号可显式选择的协议，并保留供应商顺序。
 */
export function selectableCustomModelProtocols(
  enabledProtocols: PublicAiCustomProviderProtocol[],
  routeRecord: PublicAiModelRouteRecord | undefined,
): PublicAiCustomProviderProtocol[] {
  if (!routeRecord) {
    return enabledProtocols
  }
  const availableProfiles = new Set(routeRecord.availableProtocols)
  return enabledProtocols.filter((protocol) => (
    availableProfiles.has(customProtocolProfileId(protocol))
  ))
}

// emptyProtocolRouteDraft builds one fresh draft that inherits every shared projection.
// emptyProtocolRouteDraft 构建一份全部继承共享投影的全新草稿。
export function emptyProtocolRouteDraft(): CustomProtocolRouteDraft {
  return {
    baseUrlOverride: "",
    endpointPath: "",
    authentication: "",
    requestHeaders: {},
  }
}

/**
 * Reads execution-only capability policies from the exact Codex Responses route.
 * 从精确的 Codex Responses 路由读取仅影响执行的能力策略。
 *
 * @param entry Persisted custom-provider service entry, when one exists.
 * @param entry 已持久化的自定义供应商服务入口（如果存在）。
 * @returns The exact policies, defaulting to disabled when the route or declaration is absent.
 * @returns 精确策略；当路由或声明缺失时默认为禁用。
 */
export function codexResponsesFeaturePoliciesFromEntry(
  entry: PublicAiServiceEntry | undefined,
): CodexResponsesFeaturePolicies {
  const features = entry?.protocol_routes.find(
    (route) => route.protocol_profile_id === "openai-codex-responses.v1",
  )?.responses_features
  return {
    websocketPolicy: features?.websocket ?? "disabled",
  }
}

// protocolCollectionFromEntry expands the persisted service entry into the editable protocol
// collection, preserving every per-protocol route override for a full round trip.
// protocolCollectionFromEntry 把持久化服务入口展开为可编辑协议集合，为完整往返保留每条
// 逐协议路由覆盖。
export function protocolCollectionFromEntry(
  entry: PublicAiServiceEntry | undefined,
): CustomProviderProtocolCollection {
  const drafts: CustomProviderProtocolCollection["drafts"] = {}
  const protocols: PublicAiCustomProviderProtocol[] = []
  for (const route of entry?.protocol_routes ?? []) {
    const protocol = requireCustomProtocolFromProfile(route.protocol_profile_id)
    protocols.push(protocol)
    const draft = emptyProtocolRouteDraft()
    draft.baseUrlOverride = route.base_url_override ?? ""
    draft.endpointPath = route.endpoint_path ?? ""
    if (route.authentication) {
      draft.authentication = route.authentication as CustomProtocolRouteDraft["authentication"]
    }
    if (route.request_headers) {
      draft.requestHeaders = { ...route.request_headers }
    }
    drafts[protocol] = draft
  }
  // A legacy single-protocol definition or a brand-new editor falls back to Responses.
  // 旧版单协议定义或全新编辑器回退到 Responses。
  if (protocols.length === 0) {
    protocols.push("open_ai_responses")
    drafts.open_ai_responses = emptyProtocolRouteDraft()
  }
  return {
    protocols: sortProtocolsByAutoPriority(protocols),
    drafts,
  }
}

// sortProtocolsByAutoPriority canonicalizes UI display and save order using the same published
// priority as Auto; the backend binding resolver remains the only execution authority (plan §R6.7).
// sortProtocolsByAutoPriority 使用与 Auto 相同的公开优先级规范化界面展示与保存顺序；
// 后端绑定解析器仍是唯一执行权威（方案 §R6.7）。
export function sortProtocolsByAutoPriority(
  protocols: PublicAiCustomProviderProtocol[],
): PublicAiCustomProviderProtocol[] {
  const priority = new Map<PublicAiCustomProviderProtocol, number>([
    ["open_ai_responses", 0],
    ["open_ai_codex_responses", 1],
    ["anthropic_messages", 2],
    ["open_ai_chat", 3],
    ["gemini_generate_content", 4],
  ])
  return [...protocols].sort(
    (left, right) => (priority.get(left) ?? 99) - (priority.get(right) ?? 99),
  )
}

// protocolCollectionChanged compares the complete enabled protocol set and every editable
// route field after applying the same normalization used by the save request.
// protocolCollectionChanged 使用保存请求相同的规范化方式，比较完整启用协议集合与每一项
// 可编辑路由字段。
export function protocolCollectionChanged(
  entry: PublicAiServiceEntry | undefined,
  next: CustomProviderProtocolCollection,
): boolean {
  const current = protocolCollectionFromEntry(entry)
  return protocolCollectionFingerprint(current) !== protocolCollectionFingerprint(next)
}

// protocolCollectionFingerprint returns one deterministic value for route-contract comparison;
// request-header keys are sorted because their insertion order has no protocol meaning.
// protocolCollectionFingerprint 返回用于路由契约比较的确定性值；请求头键会排序，因为
// 插入顺序不具有协议语义。
function protocolCollectionFingerprint(collection: CustomProviderProtocolCollection): string {
  const protocols = sortProtocolsByAutoPriority(collection.protocols)
  const routes = protocolRoutesFromCollection({
    protocols,
    drafts: collection.drafts,
  }).map((route) => ({
    ...route,
    request_headers: route.request_headers
      ? Object.fromEntries(Object.entries(route.request_headers).sort(([left], [right]) => left.localeCompare(right)))
      : undefined,
  }))
  return JSON.stringify({ protocols, routes })
}

// protocolRoutesFromCollection serializes the collection into the exact save-request routes;
// authentication round-trips unconditionally including an explicit bearer (plan §R6.10).
// protocolRoutesFromCollection 把集合序列化为精确保存请求路由；authentication 无条件往返，
// 包括显式 bearer（方案 §R6.10）。
export function protocolRoutesFromCollection(
  collection: CustomProviderProtocolCollection,
): PublicAiCustomProviderProtocolRoute[] {
  const routes: PublicAiCustomProviderProtocolRoute[] = []
  for (const protocol of collection.protocols) {
    const draft = collection.drafts[protocol]
    if (!draft) {
      continue
    }
    const route: PublicAiCustomProviderProtocolRoute = { protocol }
    if (draft.baseUrlOverride.trim()) {
      route.base_url = draft.baseUrlOverride.trim()
    }
    if (draft.endpointPath.trim()) {
      route.endpoint_path = draft.endpointPath.trim()
    }
    if (draft.authentication) {
      route.authentication = draft.authentication
    }
    const headers = Object.fromEntries(
      Object.entries(draft.requestHeaders).filter(([, value]) => value.trim().length > 0),
    )
    if (Object.keys(headers).length > 0) {
      route.request_headers = headers
    }
    routes.push(route)
  }
  return routes
}

// customProtocolFromProfile maps one persisted runtime profile identifier back to the browser
// protocol identifier, or null for unrecognized profiles.
// customProtocolFromProfile 把一个持久化运行时配置档标识映射回浏览器协议标识；
// 无法识别的配置档返回 null。
export function customProtocolFromProfile(
  profileId: string,
): PublicAiCustomProviderProtocol | null {
  if (profileId === "anthropic-messages.v1") {
    return "anthropic_messages"
  }
  if (profileId === "openai-chat-completions.v1") {
    return "open_ai_chat"
  }
  if (profileId === "google.gemini.generate_content") {
    return "gemini_generate_content"
  }
  if (profileId === "openai-codex-responses.v1") {
    return "open_ai_codex_responses"
  }
  if (profileId === "openai-responses.v1") {
    return "open_ai_responses"
  }
  return null
}

// requireCustomProtocolFromProfile maps one runtime profile to the closed custom-provider
// protocol contract and fails explicitly when the Service exposes an unsupported profile.
// requireCustomProtocolFromProfile 将一个运行时配置档映射到封闭的自定义供应商协议契约；
// 当 Service 暴露不受支持的配置档时显式失败。
export function requireCustomProtocolFromProfile(
  profileId: string,
): PublicAiCustomProviderProtocol {
  const protocol = customProtocolFromProfile(profileId)
  if (protocol === null) {
    throw new Error(`Unsupported custom-provider protocol profile: ${profileId}`)
  }
  return protocol
}

// customProtocolProfileId maps one browser protocol choice to its exact runtime profile.
// customProtocolProfileId 将一个浏览器协议选择映射到其精确运行时配置档。
export function customProtocolProfileId(protocol: PublicAiCustomProviderProtocol): string {
  switch (protocol) {
    case "anthropic_messages":
      return "anthropic-messages.v1"
    case "gemini_generate_content":
      return "google.gemini.generate_content"
    case "open_ai_chat":
      return "openai-chat-completions.v1"
    case "open_ai_codex_responses":
      return "openai-codex-responses.v1"
    case "open_ai_responses":
      return "openai-responses.v1"
  }
}

// customProtocolsFromServiceEntries projects the exact unique custom-provider protocol set
// from Service entries and preserves the fixed Auto-priority order.
// customProtocolsFromServiceEntries 从 Service 入口投影精确且唯一的自定义供应商协议集合，
// 并保持固定的 Auto 优先级顺序。
export function customProtocolsFromServiceEntries(
  entries: PublicAiServiceEntry[],
): PublicAiCustomProviderProtocol[] {
  const protocols = new Set<PublicAiCustomProviderProtocol>()
  for (const entry of entries) {
    for (const route of entry.protocol_routes) {
      protocols.add(requireCustomProtocolFromProfile(route.protocol_profile_id))
    }
  }
  return sortProtocolsByAutoPriority([...protocols])
}

// shouldShowCustomModelProtocolSelector allows per-model protocol selection only when the
// provider exposes more than one distinct supported protocol.
// shouldShowCustomModelProtocolSelector 仅在供应商暴露多个不同的受支持协议时，
// 允许显示逐模型协议选择器。
export function shouldShowCustomModelProtocolSelector(
  protocols: PublicAiCustomProviderProtocol[],
): boolean {
  return protocols.length > 1
}

// validateProtocolCollection rejects an empty protocol set and duplicate identifiers.
// validateProtocolCollection 拒绝空协议集合与重复标识。
export function validateProtocolCollection(
  protocols: PublicAiCustomProviderProtocol[],
): string | null {
  if (protocols.length === 0) {
    return "settings.customProviders.protocolCollectionRequired"
  }
  if (new Set(protocols).size !== protocols.length) {
    return "settings.customProviders.protocolCollectionDuplicate"
  }
  return null
}

// preferenceStillResolvable reports whether one persisted model protocol preference can still
// resolve against the enabled collection; false keeps the model blocked and requires a new
// explicit choice instead of silently falling back to Auto (plan §R6.8).
// preferenceStillResolvable 报告一条持久化模型协议偏好是否仍能在启用集合内解析；false 使
// 模型保持 blocked 并要求重新显式选择，而不是静默回退 Auto（方案 §R6.8）。
export function preferenceStillResolvable(
  preference: PublicAiCustomModelProtocolPreference,
  enabledProtocols: PublicAiCustomProviderProtocol[],
): boolean {
  if (preference.mode !== "explicit") {
    return true
  }
  return enabledProtocols.includes(preference.protocol)
}

// shouldShowCustomModelProtocolPreferenceCard exposes normal selection only for a distinct
// multi-protocol set, while retaining an Auto-reset escape hatch for a stale explicit choice.
// shouldShowCustomModelProtocolPreferenceCard 仅为不同的多协议集合暴露正常选择，同时为
// 已失效的显式选择保留重置为 Auto 的恢复入口。
export function shouldShowCustomModelProtocolPreferenceCard(
  protocols: PublicAiCustomProviderProtocol[],
  preference: PublicAiCustomModelProtocolPreference,
): boolean {
  return shouldShowCustomModelProtocolSelector(protocols)
    || !preferenceStillResolvable(preference, protocols)
}
