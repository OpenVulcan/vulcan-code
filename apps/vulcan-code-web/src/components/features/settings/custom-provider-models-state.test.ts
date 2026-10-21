import { describe, expect, it } from "vitest"
import { mediaDeliverySettingsFixture } from "@/test-fixtures/media-delivery"

import {
  accountActuallyDiscoveredModel,
  accountExposesModel,
  accountIncludesModel,
  buildCustomProviderModelRows,
  mergeCustomModelProtocolPreferences,
  shouldBlockManualModelSubmission,
  shouldReplaceManualModelDrafts,
} from "@/components/features/settings/custom-provider-models-state"
import type {
  PublicAiConnection,
  PublicAiCustomModelDefinition,
  PublicAiCustomModelProtocolPreference,
  PublicAiModel,
  PublicAiSettingsSnapshot,
} from "@/types"

// connectionFixture builds one deterministic custom-provider account.
// connectionFixture 构建一个确定性的自定义供应商账号。
function connectionFixture(id: string, displayName: string): PublicAiConnection {
  return {
    id,
    provider_id: "custom:test",
    service_entry_id: "compatible-api",
    credential_profile_id: "api-key",
    billing_kind: "pay_as_you_go",
    usage_capability: "unsupported",
    usage_requires_management_key: false,
    has_management_key: false,
    credential_mode: "token",
    credential_id: `credential-${id}`,
    display_name: displayName,
    enabled: true,
    prompt_cache_by_protocol: {},
    pending_validation: false,
  }
}

// modelFixture builds one credential-scoped model with explicit evidence.
// modelFixture 构建一个带显式证据的凭据作用域模型。
function modelFixture(id: string, displayName: string, contextLimit: number): PublicAiModel {
  return {
    id,
    display_name: displayName,
    capabilities: {
      operations: ["conversation_respond"],
      context_limit: contextLimit,
      output_limit: 4096,
      input_resources: [],
      output_resources: [],
      evidence_revision: "fixture.models.v1",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      prompt_cache: {
        provider_default_behavior: "Provider default.",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
    },
    capability_evidence: {
      context_limit_declared: true,
      output_limit_declared: true,
      input_modalities_declared: true,
      output_modalities_declared: true,
      supports_text_input: true,
      supports_text_output: true,
    },
  }
}

// manualModelFixture builds one persisted manual identity for union testing.
// manualModelFixture 构建一个用于并集测试的持久化手工模型身份。
function manualModelFixture(id: string): PublicAiCustomModelDefinition {
  return {
    id,
    display_name: "Manual Only",
    enabled: true,
    capabilities: modelFixture(id, id, 200_000).capabilities,
    request_projection: {
      reasoning: {},
      model_parameter_paths: {},
      defaults: {},
      overrides: {},
      delete: [],
    },
    tool_schema_projection: "lightweight",
    tool_mode: "direct",
  }
}

// snapshotFixture creates the smallest complete public settings snapshot accepted by the view model.
// snapshotFixture 创建视图模型接受的最小完整公共设置快照。
function snapshotFixture(): PublicAiSettingsSnapshot {
  const alpha = connectionFixture("alpha", "Alpha")
  const beta = connectionFixture("beta", "Beta")
  const alphaShared = modelFixture("shared/chat", "Shared from Alpha", 64_000)
  const betaShared = modelFixture("shared/chat", "Shared from Beta", 128_000)
  const betaOnly = modelFixture("beta/chat", "Beta Only", 32_000)
  return {
    manualModelDefaults: { contextLimit: 200_000, outputLimit: 32_000 },
    usageRefreshIntervalSeconds: 300,
    mediaDelivery: mediaDeliverySettingsFixture(),
    contextCompactionWindowNodes: {
      nodes: [
        { id: "compression-window-1", tokens: 200_000 },
        { id: "compression-window-2", tokens: 256_000 },
        { id: "compression-window-3", tokens: 272_000 },
        { id: "compression-window-4", tokens: 373_000 },
        { id: "compression-window-5", tokens: 500_000 },
      ],
    },
    newSessionContextCompactionWindowNodeId: null,
    providerPriorities: {},
    usageCache: {},
    providers: [],
    connections: [beta, alpha],
    providerDefaults: {},
    catalogs: {
      alpha: { connection_id: "alpha", models: [alphaShared], generation: 2, page_count: 1, discovery_schema_revision: "test.models.v1" },
      beta: { connection_id: "beta", models: [betaShared, betaOnly], generation: 3, page_count: 2, discovery_schema_revision: "test.models.v1" },
    },
    sourceCatalogs: {
      alpha: { connection_id: "alpha", models: [alphaShared], generation: 2, page_count: 1, discovery_schema_revision: "test.models.v1" },
      beta: { connection_id: "beta", models: [betaShared, betaOnly], generation: 3, page_count: 2, discovery_schema_revision: "test.models.v1" },
    },
    modelSupportCatalogs: {
      alpha: { connection_id: "alpha", models: [alphaShared], generation: 2, page_count: 1, discovery_schema_revision: "test.models.v1" },
      beta: { connection_id: "beta", models: [betaShared, betaOnly], generation: 3, page_count: 2, discovery_schema_revision: "test.models.v1" },
    },
    catalogStatuses: {
      alpha: { state: "ready", model_count: 1, generation: 2, observed_at_ms: 10 },
      beta: { state: "ready", model_count: 2, generation: 3, observed_at_ms: 20 },
    },
    catalogErrors: {},
    modelOverrides: {},
    modelOverrideRecords: [],
    modelAccessRecords: [
      { providerId: "custom:test", modelId: "shared/chat", enabled: true, defaultEnabled: false },
    ],
    providerToolTemplateModels: [],
    customModels: {
      "custom:test": { "manual/chat": manualModelFixture("manual/chat") },
    },
    customProviderModelDefaults: {},
    customProviderSystemModelDefaults: {
      context_limit: 200_000,
      output_limit: 32_768,
      input_resources: [],
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh"],
    },
    customModelTemplateDecisionRecords: [],
    customModelConnectionAvailabilityRecords: [
      { connectionId: "beta", modelId: "shared/chat", enabled: false },
    ],
    connectionModelAccessPolicyRecords: [],
    modelCapabilityOverrides: {},
    modelCapabilityOverrideRecords: [],
    providerToolTargets: [],
    providerToolSelections: {},
    providerToolEnabledCapabilities: [],
    providerExtensionTargetSets: {},
    internalInferenceCandidates: [],
    inferenceProfiles: {},
    inferenceRouteDiagnostics: [],
    internalModelOverrides: {},
    contextWindowPolicyOverrides: {}, contextWindowPolicyDefaults: { effective_context_window_percent: 95, auto_compact_percent: 90, auto_compact_token_limit: null, auto_compact_scope: "total", fallback_buffer_tokens: 0 },
  }
}

describe("custom provider model state", () => {
  it("unions model identities once while retaining every account fact", () => {
    const rows = buildCustomProviderModelRows(snapshotFixture(), "custom:test")
    expect(rows.map((row) => row.id)).toEqual(["beta/chat", "manual/chat", "shared/chat"])

    const shared = rows.find((row) => row.id === "shared/chat")
    expect(shared?.displayName).toBe("Shared from Alpha")
    expect(shared?.accounts).toHaveLength(2)
    expect(shared?.accounts.map((account) => account.discoveredModel?.capabilities.context_limit)).toEqual([
      64_000,
      128_000,
    ])
    expect(shared?.accounts[1].availabilityOverride).toBe(false)
  })

  it("shows synchronized source models without an import step and distinguishes effective exposure", () => {
    const rows = buildCustomProviderModelRows(snapshotFixture(), "custom:test")
    const betaOnly = rows.find((row) => row.id === "beta/chat")
    expect(betaOnly).toBeDefined()
    expect(accountActuallyDiscoveredModel(betaOnly!.accounts[1])).toBe(true)
    expect(accountExposesModel(betaOnly!.accounts[1])).toBe(true)
    expect(betaOnly?.manualDefinition).toBeUndefined()
  })

  it("retains provider-resolved capabilities when account access hides every model", () => {
    const snapshot = snapshotFixture()
    snapshot.catalogs = {
      alpha: { ...snapshot.catalogs.alpha!, models: [] },
      beta: { ...snapshot.catalogs.beta!, models: [] },
    }

    const shared = buildCustomProviderModelRows(snapshot, "custom:test")
      .find((row) => row.id === "shared/chat")

    expect(shared?.effectiveModel).toBeUndefined()
    expect(shared?.capabilityModel?.capabilities.context_limit).toBe(64_000)
    expect(shared?.accounts.every((account) => !accountExposesModel(account))).toBe(true)
    expect(shared?.accounts.every(accountIncludesModel)).toBe(true)
    expect(shared?.accounts[0].supportModel?.capabilities.context_limit).toBe(64_000)
  })

  it("uses an explicit user display name without replacing account discovery facts", () => {
    const snapshot = snapshotFixture()
    snapshot.customModels!["custom:test"]!["shared/chat"] = {
      ...manualModelFixture("shared/chat"),
      display_name: "User Model Name",
    }

    const shared = buildCustomProviderModelRows(snapshot, "custom:test")
      .find((row) => row.id === "shared/chat")

    expect(shared?.displayName).toBe("User Model Name")
    expect(shared?.accounts[0].discoveredModel?.display_name).toBe("Shared from Alpha")
    expect(shared?.accounts[1].discoveredModel?.display_name).toBe("Shared from Beta")
  })

  it("preserves dirty advanced drafts across unrelated service snapshots", () => {
    expect(shouldReplaceManualModelDrafts(true, false)).toBe(false)
    expect(shouldReplaceManualModelDrafts(true, true)).toBe(true)
    expect(shouldReplaceManualModelDrafts(false, false)).toBe(true)
  })

  it("blocks manual model submission until template lookup and persistence are idle", () => {
    expect(shouldBlockManualModelSubmission(false, true)).toBe(true)
    expect(shouldBlockManualModelSubmission(true, false)).toBe(true)
    expect(shouldBlockManualModelSubmission(false, false)).toBe(false)
  })

  // Preserves untouched model preferences when one card sends a provider-wide replacement.
  // 当一个卡片发送供应商级替换时保留未编辑模型的偏好。
  it("merges one protocol edit into the complete sparse provider preference set", () => {
    const current = new Map<string, PublicAiCustomModelProtocolPreference>([
      ["model-a", { mode: "explicit", protocol: "open_ai_responses" }],
      ["model-b", { mode: "explicit", protocol: "anthropic_messages" } as const],
      ["model-c", { mode: "auto" } as const],
    ])

    expect(mergeCustomModelProtocolPreferences(current, [{
      modelId: "model-a",
      preference: { mode: "auto" },
    }])).toEqual([
      {
        modelId: "model-b",
        preference: { mode: "explicit", protocol: "anthropic_messages" },
      },
    ])
    expect(mergeCustomModelProtocolPreferences(current, [{
      modelId: "model-b",
      preference: { mode: "explicit", protocol: "gemini_generate_content" },
    }])).toEqual([
      {
        modelId: "model-a",
        preference: { mode: "explicit", protocol: "open_ai_responses" },
      },
      {
        modelId: "model-b",
        preference: { mode: "explicit", protocol: "gemini_generate_content" },
      },
    ])
  })
})
