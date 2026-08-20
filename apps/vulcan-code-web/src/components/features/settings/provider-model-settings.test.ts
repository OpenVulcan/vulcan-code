import { describe, expect, it } from "vitest"

import {
  internalStateForModel,
  multimodalCandidateToggleDisabled,
} from "@/components/features/settings/provider-model-settings"
import type { PublicAiInternalInferenceCandidate } from "@/types"

// candidateFixture builds one exact internal route while varying operation, scope, and state.
// candidateFixture 构建一条精确内部路由，并允许改变操作、用途与状态。
function candidateFixture(
  operation: PublicAiInternalInferenceCandidate["route"]["operation"],
  usageScope: PublicAiInternalInferenceCandidate["route"]["usage_scope"],
  enabled: boolean,
): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "OpenCode Go",
    serviceEntryDisplayName: "Go API",
    connectionDisplayName: "Test account",
    modelDisplayName: "DeepSeek V4 Flash",
    route: {
      provider_id: "opencode-go",
      service_entry_id: "go-api",
      connection_id: "go-account",
      model_id: "deepseek-v4-flash",
      binding: { kind: "protocol", protocol_profile_id: "openai-responses.v1" },
      operation,
      usage_scope: usageScope,
    },
    catalogGeneration: 1,
    enabled,
    usageScopes: ["internal_background", "external_service"],
    capabilities: {
      operations: ["conversation_respond"],
      input_resources: [],
      output_resources: [],
      evidence_revision: "test",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      prompt_cache: {
        provider_default_behavior: "test",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
    },
    overrides: {},
  }
}

describe("internalStateForModel", () => {
  it("collapses operations and runtime scopes into one model-level internal switch", () => {
    const state = internalStateForModel({
      internalInferenceCandidates: [
        candidateFixture("conversation_respond", "internal_background", true),
        candidateFixture("conversation_respond", "external_service", true),
        candidateFixture("embedding_create", "internal_background", true),
      ],
    }, "opencode-go", "deepseek-v4-flash")

    expect(state).toEqual({
      enabled: true,
      operations: ["conversation_respond", "embedding_create"],
    })
  })

  it("returns no switch when the model has no reachable internal route", () => {
    expect(internalStateForModel({
      internalInferenceCandidates: [],
    }, "opencode-go", "deepseek-v4-flash")).toBeNull()
  })
})

describe("multimodalCandidateToggleDisabled", () => {
  it("allows an admitted model to be removed after its route becomes ineligible", () => {
    expect(multimodalCandidateToggleDisabled(false, false, true)).toBe(false)
  })

  it("blocks a new admission without an eligible executable route", () => {
    expect(multimodalCandidateToggleDisabled(false, false, false)).toBe(true)
  })

  it("blocks every candidate mutation while another settings write is pending", () => {
    expect(multimodalCandidateToggleDisabled(true, true, true)).toBe(true)
  })
})
