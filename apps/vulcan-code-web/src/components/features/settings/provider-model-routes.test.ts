import { describe, expect, it } from "vitest"

import {
  buildProviderModelInventoryGroups,
  buildProviderModelGroups,
  compactServiceEntryName,
} from "@/components/features/settings/provider-model-routes"
import type {
  PublicAiConnection,
  PublicAiConnectionCatalog,
  PublicAiProviderDescriptor,
} from "@/types"

// providerFixture declares two independently configurable service entries sharing one protocol.
// providerFixture 声明两个共享同一协议但可独立配置的服务入口。
const providerFixture: PublicAiProviderDescriptor = {
  id: "relay",
  display_name: "Relay",
  kind: "system",
  category: "relay",
  credential_directory_visibility: "configured_only",
  credential_registration: "direct",
  service_entries: [
    {
      id: "default-api",
      display_name: "Default API",
      kind: "conversation",
      base_url: "https://default.example/v1",
      credential_profiles: [],
      authentication: "bearer_authorization",
      model_discovery: "open_ai",
      protocol_routes: [],
    },
    {
      id: "fast-api",
      display_name: "Fast API",
      kind: "conversation",
      base_url: "https://fast.example/v1",
      credential_profiles: [],
      authentication: "bearer_authorization",
      model_discovery: "open_ai",
      protocol_routes: [],
    },
  ],
}

// connectionFixture builds one configured account for one exact service entry.
// connectionFixture 为一个精确服务入口构建一个已配置账号。
function connectionFixture(id: string, serviceEntryId: string): PublicAiConnection {
  return {
    id,
    provider_id: providerFixture.id,
    service_entry_id: serviceEntryId,
    credential_profile_id: "api-key",
    billing_kind: "pay_as_you_go",
    usage_capability: "unsupported",
    usage_requires_management_key: false,
    has_management_key: false,
    credential_mode: "token",
    credential_id: `credential-${id}`,
    display_name: id,
    enabled: true,
    prompt_cache_by_protocol: {},
    pending_validation: false,
  }
}

// catalogFixture builds one route-local catalog containing the shared test model.
// catalogFixture 构建一个包含共享测试模型的路由本地目录。
function catalogFixture(connectionId: string): PublicAiConnectionCatalog {
  return {
    connection_id: connectionId,
    generation: 1,
    page_count: 1,
    discovery_schema_revision: "test.models.v1",
    models: [{
      id: "model-1",
      display_name: "Model 1",
      capabilities: {
        evidence_revision: "test",
        reasoning: "unsupported",
        prompt_cache: {
          provider_default_behavior: "provider_default",
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
      },
    }],
  }
}

// modelRouteFixture supplies one backend-authoritative route record.
// modelRouteFixture 提供一条后端权威路由记录。
function modelRouteFixture(
  connectionId: string,
  serviceEntryId: string,
  protocolId = "openai-chat-completions.v1",
) {
  return {
    providerId: providerFixture.id,
    connectionId,
    serviceEntryId,
    modelId: "model-1",
    protocolId,
    availableProtocols: [protocolId],
    bindingSource: "system_manifest" as const,
    bindingRuleId: "model-1",
    manifestRevision: 1,
    executable: true,
  }
}

describe("buildProviderModelGroups", () => {
  it("keeps configured service entries separate even when they use the same protocol", () => {
    const defaultConnection = connectionFixture("default-account", "default-api")
    const fastConnection = connectionFixture("fast-account", "fast-api")
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [defaultConnection, fastConnection],
      catalogs: {
        [defaultConnection.id]: catalogFixture(defaultConnection.id),
        [fastConnection.id]: catalogFixture(fastConnection.id),
      },
      modelRouteRecords: [
        modelRouteFixture(defaultConnection.id, "default-api"),
        modelRouteFixture(fastConnection.id, "fast-api"),
      ],
      modelSupportCatalogs: {
        [defaultConnection.id]: catalogFixture(defaultConnection.id),
        [fastConnection.id]: catalogFixture(fastConnection.id),
      },
    })

    expect(groups[0]?.models.get("model-1")).toMatchObject([
      { serviceEntryId: "default-api", serviceEntryName: "Default API", accountCount: 1, accounts: [{ connectionId: "default-account", displayName: "default-account", enabled: true }] },
      { serviceEntryId: "fast-api", serviceEntryName: "Fast API", accountCount: 1, accounts: [{ connectionId: "fast-account", displayName: "fast-account", enabled: true }] },
    ])
  })

  it("does not render provider service entries that have no configured account catalog", () => {
    const defaultConnection = connectionFixture("default-account", "default-api")
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [defaultConnection],
      catalogs: { [defaultConnection.id]: catalogFixture(defaultConnection.id) },
      modelRouteRecords: [modelRouteFixture(defaultConnection.id, "default-api")],
      modelSupportCatalogs: { [defaultConnection.id]: catalogFixture(defaultConnection.id) },
    })

    expect(groups[0]?.models.get("model-1")).toHaveLength(1)
    expect(groups[0]?.models.get("model-1")?.[0]?.serviceEntryId).toBe("default-api")
  })

  it("keeps a model manageable when every account has disabled it", () => {
    const connection = connectionFixture("disabled-account", "default-api")
    const disabledCatalog = {
      ...catalogFixture(connection.id),
      models: [],
    }
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: disabledCatalog },
      modelRouteRecords: [modelRouteFixture(connection.id, "default-api")],
      modelSupportCatalogs: { [connection.id]: catalogFixture(connection.id) },
    })

    expect(groups[0]?.models.get("model-1")?.[0]?.accountCount).toBe(0)
    expect(groups[0]?.models.get("model-1")?.[0]?.accounts).toEqual([{ connectionId: "disabled-account", displayName: "disabled-account", enabled: false }])
  })

  it("keeps a conversation-disabled model visible for independent model management", () => {
    const connection = connectionFixture("provider-disabled-account", "default-api")
    const snapshot = {
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: catalogFixture(connection.id) },
      modelAccessRecords: [{
        providerId: providerFixture.id,
        modelId: "model-1",
        enabled: false,
        defaultEnabled: true,
      }],
      modelRouteRecords: [modelRouteFixture(connection.id, "default-api")],
      modelSupportCatalogs: { [connection.id]: catalogFixture(connection.id) },
    }
    const groups = buildProviderModelGroups(snapshot)

    expect(groups[0]?.models.get("model-1")).toHaveLength(1)
  })

  it("uses the backend model binding when it differs from the connection default", () => {
    const connection = connectionFixture("responses-model-account", "default-api")
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: catalogFixture(connection.id) },
      modelRouteRecords: [
        modelRouteFixture(connection.id, "default-api", "openai-responses.v1"),
      ],
      modelSupportCatalogs: { [connection.id]: catalogFixture(connection.id) },
    })

    expect(groups[0]?.models.get("model-1")?.[0]?.protocolId).toBe("openai-responses.v1")
    expect(groups[0]?.models.get("model-1")?.[0]).toMatchObject({
      availableProtocols: ["openai-responses.v1"],
      bindingSource: "system_manifest",
      bindingRuleId: "model-1",
      manifestRevision: 1,
    })
  })

  // Verifies a backend-blocked route cannot become a model settings execution route.
  // 验证后端阻止的路由不能变成模型设置中的执行路由。
  it("excludes backend-blocked model routes", () => {
    const connection = connectionFixture("blocked-model-account", "default-api")
    const blockedRecord = {
      ...modelRouteFixture(connection.id, "default-api"),
      protocolId: undefined,
      bindingSource: undefined,
      executable: false,
      unavailableReason: "No model-level protocol binding.",
    }
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: catalogFixture(connection.id) },
      modelRouteRecords: [blockedRecord],
      modelSupportCatalogs: { [connection.id]: catalogFixture(connection.id) },
    })

    expect(groups).toHaveLength(0)
  })

  // Verifies accounts with different binding provenance remain independently explainable.
  // 验证绑定溯源不同的账号保持独立且可解释。
  it("does not merge accounts whose rule provenance differs", () => {
    // firstConnection and secondConnection share one service and protocol but not one rule.
    // firstConnection 与 secondConnection 共享服务和协议，但不共享规则。
    const firstConnection = connectionFixture("rule-a-account", "default-api")
    const secondConnection = connectionFixture("rule-b-account", "default-api")
    // secondRecord changes only the backend rule identity.
    // secondRecord 只改变后端规则身份。
    const secondRecord = {
      ...modelRouteFixture(secondConnection.id, "default-api"),
      bindingRuleId: "model-1-credential-b",
    }
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [firstConnection, secondConnection],
      catalogs: {
        [firstConnection.id]: catalogFixture(firstConnection.id),
        [secondConnection.id]: catalogFixture(secondConnection.id),
      },
      modelRouteRecords: [
        modelRouteFixture(firstConnection.id, "default-api"),
        secondRecord,
      ],
      modelSupportCatalogs: {
        [firstConnection.id]: catalogFixture(firstConnection.id),
        [secondConnection.id]: catalogFixture(secondConnection.id),
      },
    })

    expect(groups[0]?.models.get("model-1")).toMatchObject([
      { accountCount: 1, bindingRuleId: "model-1" },
      { accountCount: 1, bindingRuleId: "model-1-credential-b" },
    ])
  })
})

describe("buildProviderModelInventoryGroups", () => {
  it("keeps native embedding and signed image or video models visible without conversation protocol routes", () => {
    const connection = connectionFixture("media-account", "default-api")
    const baseModel = catalogFixture(connection.id).models[0]!
    const embedding = {
      ...baseModel,
      id: "vector-model",
      display_name: "Vector Model",
      capabilities: { ...baseModel.capabilities, operations: ["embedding_create" as const] },
    }
    const supportCatalog = { ...catalogFixture(connection.id), models: [embedding] }
    const groups = buildProviderModelInventoryGroups({
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: supportCatalog },
      modelSupportCatalogs: { [connection.id]: supportCatalog },
      modelRouteRecords: [],
      providerToolTemplateModels: [{
        providerId: providerFixture.id,
        connectionId: connection.id,
        serviceEntryId: "default-api",
        modelId: "image-model",
        displayName: "Image Model",
      }, {
        providerId: providerFixture.id,
        connectionId: connection.id,
        serviceEntryId: "default-api",
        modelId: "video-model",
        displayName: "Video Model",
        capabilities: ["video_generate"],
      }],
      providerToolTargets: [{
        connectionId: connection.id,
        connectionDisplayName: connection.display_name,
        manifestSourceLayer: "bundled",
        target: {
          target_id: "image-target",
          provider_id: providerFixture.id,
          service_entry_id: "default-api",
          credential_profile_ids: ["api-key"],
          upstream_target_id: "image-model",
          display_name: "Image Model",
          contract: {
            tool_id: "image_generate",
            description: "Generate image",
            capability: "image_generate",
            input_schema: {},
            output: { kind: "resources", source_path: "images", normalized_result_path: "images" },
          },
        },
      }],
    })

    expect([...groups[0]!.models.keys()]).toEqual(["vector-model", "image-model", "video-model"])
    expect(groups[0]!.models.get("vector-model")).toMatchObject({ availableAccountCount: 1, protocolRoutes: [] })
    expect(groups[0]!.models.get("image-model")).toMatchObject({ availableAccountCount: 1, extensionCapabilities: ["image_generate"] })
    expect(groups[0]!.models.get("video-model")).toMatchObject({ availableAccountCount: 0, extensionCapabilities: ["video_generate"] })
  })
})

describe("compactServiceEntryName", () => {
  it("keeps route tabs compact without changing unrelated names", () => {
    expect(compactServiceEntryName("Token Plan Personal China")).toBe("Token Plan CN")
    expect(compactServiceEntryName("Token Plan Personal International")).toBe("Token Plan Intl")
    expect(compactServiceEntryName("Model Studio Singapore")).toBe("Model Studio SG")
    expect(compactServiceEntryName("Default API")).toBe("Default API")
  })
})
