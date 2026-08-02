import { describe, expect, it } from "vitest"

import {
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
    has_management_key: false,
    credential_mode: "token",
    credential_id: `credential-${id}`,
    display_name: id,
    verified_protocols: ["openai-chat-completions.v1"],
    selected_protocol: "openai-chat-completions.v1",
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
        tool_calling: "unsupported",
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
      modelSupportCatalogs: {
        [defaultConnection.id]: catalogFixture(defaultConnection.id),
        [fastConnection.id]: catalogFixture(fastConnection.id),
      },
    })

    expect(groups[0]?.models.get("model-1")).toMatchObject([
      { serviceEntryId: "default-api", serviceEntryName: "Default API", accountCount: 1 },
      { serviceEntryId: "fast-api", serviceEntryName: "Fast API", accountCount: 1 },
    ])
  })

  it("does not render provider service entries that have no configured account catalog", () => {
    const defaultConnection = connectionFixture("default-account", "default-api")
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [defaultConnection],
      catalogs: { [defaultConnection.id]: catalogFixture(defaultConnection.id) },
      modelSupportCatalogs: { [defaultConnection.id]: catalogFixture(defaultConnection.id) },
    })

    expect(groups[0]?.models.get("model-1")).toHaveLength(1)
    expect(groups[0]?.models.get("model-1")?.[0]?.serviceEntryId).toBe("default-api")
  })

  it("keeps a supported model visible when every account has disabled it", () => {
    const connection = connectionFixture("disabled-account", "default-api")
    const disabledCatalog = {
      ...catalogFixture(connection.id),
      models: [],
    }
    const groups = buildProviderModelGroups({
      providers: [providerFixture],
      connections: [connection],
      catalogs: { [connection.id]: disabledCatalog },
      modelSupportCatalogs: { [connection.id]: catalogFixture(connection.id) },
    })

    expect(groups[0]?.models.get("model-1")).toMatchObject([
      { serviceEntryId: "default-api", accountCount: 0 },
    ])
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
