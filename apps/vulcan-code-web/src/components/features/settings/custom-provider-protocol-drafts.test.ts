import { describe, expect, it } from "vitest"

import {
  CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE,
  codexResponsesFeaturePoliciesFromEntry,
  customProtocolRouteDraftValue,
  customProtocolRouteSelectValue,
  customProtocolProfileId,
  customProtocolsFromServiceEntries,
  emptyProtocolRouteDraft,
  exactCustomModelRouteRecord,
  preferenceStillResolvable,
  protocolCollectionChanged,
  protocolCollectionFromEntry,
  protocolRoutesFromCollection,
  selectableCustomModelProtocols,
  sortProtocolsByAutoPriority,
  shouldShowCustomModelProtocolPreferenceCard,
  shouldShowCustomModelProtocolSelector,
  validateProtocolCollection,
} from "@/components/features/settings/custom-provider-protocol-drafts"
import type {
  PublicAiCustomProviderProtocol,
  PublicAiModelRouteRecord,
  PublicAiServiceEntry,
} from "@/types"

// entryFixture builds one multi-protocol service entry with distinct route overrides.
// entryFixture 构建一个带差异化路由覆盖的多协议服务入口。
function entryFixture(): PublicAiServiceEntry {
  return {
    id: "default",
    display_name: "Compatible API",
    kind: "conversation",
    base_url: "https://example.test/v1",
    credential_profiles: [],
    authentication: "bearer_authorization",
    model_discovery: "open_ai",
    protocol_routes: [
      {
        protocol_profile_id: "openai-responses.v1",
        endpoint_path: "/responses",
        base_url_override: "https://responses.example.test",
        request_headers: { accept: "application/json" },
        authentication: "x_api_key",
        model_discovery: "open_ai",
        model_discovery_path: undefined,
        responses_features: undefined,
        verified: true,
      },
      {
        protocol_profile_id: "anthropic-messages.v1",
        endpoint_path: "/messages",
        base_url_override: undefined,
        request_headers: {},
        authentication: undefined,
        model_discovery: "anthropic",
        model_discovery_path: undefined,
        responses_features: undefined,
        verified: true,
      },
    ],
  }
}

describe("custom provider protocol drafts", () => {
  // Verifies the Select UI never receives Radix's reserved empty item value while persistence
  // continues to use the established empty-string inheritance contract.
  // 验证 Select UI 绝不接收 Radix 保留的空选项值，同时持久化继续使用既有的空字符串继承契约。
  it("round trips inherited route fields through a non-empty Select sentinel", () => {
    expect(CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE).not.toBe("")
    expect(customProtocolRouteSelectValue("")).toBe(CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE)
    expect(customProtocolRouteDraftValue(CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE)).toBe("")
    expect(customProtocolRouteSelectValue("x_api_key")).toBe("x_api_key")
    expect(customProtocolRouteDraftValue("x_api_key")).toBe("x_api_key")
  })

  // Verifies route diagnostics use the selected credential identity and never borrow a sibling route.
  // 验证路由诊断使用选定凭据身份，且绝不借用同级账号路由。
  it("selects only the exact backend route record for the chosen account", () => {
    // records contains two independently resolved accounts for the same provider model.
    // records 包含同一供应商模型下两条独立解析的账号记录。
    const records: PublicAiModelRouteRecord[] = [
      {
        providerId: "custom-relay",
        connectionId: "account-a",
        serviceEntryId: "default",
        modelId: "model-1",
        protocolId: "openai-responses.v1",
        availableProtocols: ["openai-responses.v1", "anthropic-messages.v1"],
        bindingSource: "custom_auto_default",
        bindingRuleId: "auto:model-1",
        manifestRevision: 3,
        executable: true,
      },
      {
        providerId: "custom-relay",
        connectionId: "account-b",
        serviceEntryId: "default",
        modelId: "model-1",
        protocolId: "anthropic-messages.v1",
        availableProtocols: ["anthropic-messages.v1"],
        bindingSource: "custom_user",
        manifestRevision: 3,
        executable: true,
      },
    ]

    expect(exactCustomModelRouteRecord(
      records,
      "custom-relay",
      "model-1",
      "account-b",
    )).toBe(records[1])
    expect(exactCustomModelRouteRecord(
      records,
      "custom-relay",
      "model-1",
      "missing-account",
    )).toBeUndefined()
    expect(exactCustomModelRouteRecord(
      records,
      "custom-relay",
      "model-1",
      null,
    )).toBeUndefined()
  })

  // Verifies explicit choices use exact backend model variants rather than provider-wide support.
  // 验证显式选择使用精确后端模型变体，而不是供应商全局支持集合。
  it("limits explicit protocol choices to the selected model route", () => {
    const record: PublicAiModelRouteRecord = {
      providerId: "custom-relay",
      connectionId: "account-a",
      serviceEntryId: "default",
      modelId: "anthropic-only/model",
      availableProtocols: ["anthropic-messages.v1"],
      manifestRevision: 4,
      executable: false,
      unavailableReason: "The explicit Responses route is unavailable for this model.",
    }
    const enabled: PublicAiCustomProviderProtocol[] = [
      "open_ai_responses",
      "anthropic_messages",
      "open_ai_chat",
    ]

    expect(selectableCustomModelProtocols(enabled, record)).toEqual([
      "anthropic_messages",
    ])
    expect(selectableCustomModelProtocols(enabled, undefined)).toEqual(enabled)
  })

  it("expands every persisted route into its own draft preserving overrides", () => {
    const collection = protocolCollectionFromEntry(entryFixture())
    expect(collection.protocols).toEqual([
      "open_ai_responses",
      "anthropic_messages",
    ])
    const responsesDraft = collection.drafts.open_ai_responses
    expect(responsesDraft?.baseUrlOverride).toBe("https://responses.example.test")
    expect(responsesDraft?.endpointPath).toBe("/responses")
    expect(responsesDraft?.authentication).toBe("x_api_key")
    expect(responsesDraft?.requestHeaders.accept).toBe("application/json")
    const anthropicDraft = collection.drafts.anthropic_messages
    expect(anthropicDraft?.baseUrlOverride).toBe("")
    expect(anthropicDraft?.authentication).toBe("")
  })

  it("falls back to a single Responses protocol for a missing entry", () => {
    const collection = protocolCollectionFromEntry(undefined)
    expect(collection.protocols).toEqual(["open_ai_responses"])
    expect(collection.drafts.open_ai_responses).toEqual(emptyProtocolRouteDraft())
  })

  // Verifies execution-only capabilities are read solely from the exact Codex Responses route.
  // 验证仅影响执行的能力只从精确 Codex Responses 路由读取。
  it("reads Codex Responses feature policies from their unique authoritative route", () => {
    const entry = entryFixture()
    entry.protocol_routes.push({
      protocol_profile_id: "openai-codex-responses.v1",
      endpoint_path: "/codex/responses",
      request_headers: {},
      model_discovery: "open_ai",
      responses_features: {
        websocket: "unverified_override",
      },
      verified: true,
    })

    expect(codexResponsesFeaturePoliciesFromEntry(entry)).toEqual({
      websocketPolicy: "unverified_override",
    })
    expect(codexResponsesFeaturePoliciesFromEntry(entryFixture())).toEqual({
      websocketPolicy: "disabled",
    })
  })

  it("serializes routes with full authentication round trip including explicit bearer", () => {
    const collection = protocolCollectionFromEntry(entryFixture())
    const routes = protocolRoutesFromCollection(collection)
    expect(routes).toHaveLength(2)
    const responses = routes.find((route) => route.protocol === "open_ai_responses")
    expect(responses?.authentication).toBe("x_api_key")
    expect(responses?.base_url).toBe("https://responses.example.test")
    expect(responses?.endpoint_path).toBe("/responses")
    expect(responses).not.toHaveProperty("model_discovery_mode")
    expect(responses).not.toHaveProperty("model_discovery_path")
    expect(responses?.request_headers).toEqual({ accept: "application/json" })
    expect(responses).not.toHaveProperty("baseUrl")
    expect(responses).not.toHaveProperty("endpointPath")
    expect(responses).not.toHaveProperty("modelDiscoveryMode")
    expect(responses).not.toHaveProperty("requestHeaders")
    const anthropic = routes.find((route) => route.protocol === "anthropic_messages")
    expect(anthropic?.authentication).toBeUndefined()
    expect(anthropic?.base_url).toBeUndefined()
  })

  // Verifies persistence emits exactly the enabled route set for both single- and multi-protocol providers.
  // 验证单协议与多协议供应商的持久化都只输出精确启用的路由集合。
  it("serializes the exact route cardinality for single and multi protocol providers", () => {
    // singleRoutes is the unique default Responses route for a new provider.
    // singleRoutes 是新供应商唯一的默认 Responses 路由。
    const singleRoutes = protocolRoutesFromCollection(protocolCollectionFromEntry(undefined))
    expect(singleRoutes).toHaveLength(1)
    expect(singleRoutes[0]?.protocol).toBe("open_ai_responses")

    // multiRoutes preserves every independently configured persisted route.
    // multiRoutes 保留每一条独立配置并持久化的路由。
    const multiRoutes = protocolRoutesFromCollection(protocolCollectionFromEntry(entryFixture()))
    expect(multiRoutes).toHaveLength(2)
    expect(multiRoutes.map((route) => route.protocol)).toEqual([
      "open_ai_responses",
      "anthropic_messages",
    ])
  })

  it("detects additions and edits on every protocol route", () => {
    const original = entryFixture()
    const unchanged = protocolCollectionFromEntry(original)
    expect(protocolCollectionChanged(original, unchanged)).toBe(false)

    const withAddedProtocol = {
      protocols: sortProtocolsByAutoPriority([...unchanged.protocols, "open_ai_chat"]),
      drafts: {
        ...unchanged.drafts,
        open_ai_chat: emptyProtocolRouteDraft(),
      },
    } satisfies ReturnType<typeof protocolCollectionFromEntry>
    expect(protocolCollectionChanged(original, withAddedProtocol)).toBe(true)

    const changedSecondaryRoute = protocolCollectionFromEntry(original)
    changedSecondaryRoute.drafts.anthropic_messages = {
      ...changedSecondaryRoute.drafts.anthropic_messages!,
      endpointPath: "/v2/messages",
    }
    expect(protocolCollectionChanged(original, changedSecondaryRoute)).toBe(true)
  })

  it("keeps Auto protocol priority stable after collection edits", () => {
    expect(sortProtocolsByAutoPriority([
      "open_ai_chat",
      "anthropic_messages",
      "open_ai_responses",
      "gemini_generate_content",
    ])).toEqual([
      "open_ai_responses",
      "anthropic_messages",
      "open_ai_chat",
      "gemini_generate_content",
    ])
  })

  // Verifies Service runtime profile IDs become the exact persisted enum values once, even
  // when multiple entries expose the same protocol.
  // 验证 Service 运行时配置档标识会精确转换为持久化枚举值，且多个入口暴露同一协议时
  // 只保留一次。
  it("maps, deduplicates, and orders model-selectable protocols", () => {
    // duplicateResponsesEntry repeats only the Responses route under another service entry.
    // duplicateResponsesEntry 在另一个服务入口下仅重复 Responses 路由。
    const duplicateResponsesEntry: PublicAiServiceEntry = {
      ...entryFixture(),
      id: "secondary",
      protocol_routes: [entryFixture().protocol_routes[0]!],
    }
    // protocols is the browser-persistable set used by the per-model selector.
    // protocols 是逐模型选择器使用的浏览器可持久化集合。
    const protocols = customProtocolsFromServiceEntries([
      duplicateResponsesEntry,
      entryFixture(),
    ])
    expect(protocols).toEqual(["open_ai_responses", "anthropic_messages"])
    expect(customProtocolProfileId(protocols[0]!)).toBe("openai-responses.v1")
    expect(customProtocolProfileId(protocols[1]!)).toBe("anthropic-messages.v1")
  })

  // Verifies selector visibility is based on distinct supported protocols rather than route count.
  // 验证选择器可见性依据不同受支持协议，而不是路由数量。
  it("shows the model protocol selector only for a distinct multi-protocol set", () => {
    expect(shouldShowCustomModelProtocolSelector([])).toBe(false)
    expect(shouldShowCustomModelProtocolSelector(["open_ai_responses"])).toBe(false)
    expect(shouldShowCustomModelProtocolSelector([
      "open_ai_responses",
      "anthropic_messages",
    ])).toBe(true)
  })

  // Verifies a single-protocol provider exposes no selector but still offers an explicit
  // recovery action when a removed protocol leaves one model intentionally blocked.
  // 验证单协议供应商不暴露选择器，但当已删除协议刻意使模型阻塞时，仍提供显式恢复入口。
  it("keeps an Auto recovery card for a stale explicit preference", () => {
    // singleProtocol is the only exact route still enabled after a provider edit.
    // singleProtocol 是供应商编辑后仍启用的唯一精确路由。
    const singleProtocol: PublicAiCustomProviderProtocol[] = ["open_ai_responses"]
    expect(shouldShowCustomModelProtocolPreferenceCard(
      singleProtocol,
      { mode: "auto" },
    )).toBe(false)
    expect(shouldShowCustomModelProtocolPreferenceCard(
      singleProtocol,
      { mode: "explicit", protocol: "open_ai_responses" },
    )).toBe(false)
    expect(shouldShowCustomModelProtocolPreferenceCard(
      singleProtocol,
      { mode: "explicit", protocol: "open_ai_chat" },
    )).toBe(true)
  })

  // Verifies an unknown runtime profile cannot be reinterpreted as Responses or another protocol.
  // 验证未知运行时配置档不能被重新解释为 Responses 或其他协议。
  it("fails closed for an unsupported custom-provider runtime profile", () => {
    // invalidEntry represents a Service projection outside the closed custom protocol contract.
    // invalidEntry 表示超出封闭自定义协议契约的 Service 投影。
    const invalidEntry: PublicAiServiceEntry = {
      ...entryFixture(),
      protocol_routes: [{
        ...entryFixture().protocol_routes[0]!,
        protocol_profile_id: "future-unknown.v1",
      }],
    }
    expect(() => protocolCollectionFromEntry(invalidEntry)).toThrow(
      "Unsupported custom-provider protocol profile: future-unknown.v1",
    )
    expect(() => customProtocolsFromServiceEntries([invalidEntry])).toThrow(
      "Unsupported custom-provider protocol profile: future-unknown.v1",
    )
  })

  it("rejects an empty collection and duplicate identifiers", () => {
    expect(validateProtocolCollection([])).toBe(
      "settings.customProviders.protocolCollectionRequired",
    )
    expect(
      validateProtocolCollection(["open_ai_responses", "open_ai_responses"]),
    ).toBe("settings.customProviders.protocolCollectionDuplicate")
    expect(
      validateProtocolCollection(["open_ai_responses", "open_ai_chat"]),
    ).toBeNull()
  })

  it("keeps an explicit preference blocked when its protocol is removed", () => {
    const enabled: PublicAiCustomProviderProtocol[] = ["open_ai_responses"]
    expect(
      preferenceStillResolvable({ mode: "explicit", protocol: "open_ai_chat" }, enabled),
    ).toBe(false)
    expect(
      preferenceStillResolvable({ mode: "explicit", protocol: "open_ai_responses" }, enabled),
    ).toBe(true)
    expect(preferenceStillResolvable({ mode: "auto" }, enabled)).toBe(true)
  })
})
