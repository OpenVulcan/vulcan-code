import { describe, expect, it } from "vitest"

import {
  advancedModelTextDraftFromDefinition,
  applyAdvancedModelTextDrafts,
} from "@/components/features/settings/custom-provider-model-drafts"
import type { PublicAiCustomModelDefinition } from "@/types"

// modelFixture builds one strict manual definition for advanced-draft tests.
// modelFixture 为高级草稿测试构建一项严格手工定义。
function modelFixture(id: string): PublicAiCustomModelDefinition {
  return {
    id,
    display_name: id,
    enabled: true,
    capabilities: {
      operations: ["conversation_respond"],
      input_resources: [],
      output_resources: [],
      evidence_revision: "test",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      tool_calling: "unknown",
      parallel_tool_calling: "unknown",
      prompt_cache: {
        provider_default_behavior: "test",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
    },
    request_projection: {
      reasoning: {},
      model_parameter_paths: {},
      defaults: {},
      overrides: {},
      delete: [],
    },
  }
}

describe("custom provider advanced model drafts", () => {
  it("applies independent drafts for every edited model", () => {
    const first = modelFixture("first")
    const second = modelFixture("second")
    const untouched = modelFixture("untouched")
    const firstDraft = advancedModelTextDraftFromDefinition(first)
    const secondDraft = advancedModelTextDraftFromDefinition(second)
    firstDraft.projectionText = JSON.stringify({
      reasoning: {
        high: {
          set: { "reasoning.enabled": true },
          delete: [],
        },
      },
      model_parameter_paths: {},
      defaults: {},
      overrides: {},
      delete: [],
    })
    secondDraft.parameterText = JSON.stringify([{
      id: "temperature",
      display_name: "Temperature",
      description: "Sampling temperature",
      schema: { type: "number" },
      default: { type: "provider_default" },
      protocol_ids: [],
      model_ids: [],
      dependent_required: [],
      conflicts: [],
      evidence_revision: "test",
    }])

    const result = applyAdvancedModelTextDrafts(
      [first, second, untouched],
      { first: firstDraft, second: secondDraft },
      { projectionInvalid: "projection invalid", parametersInvalid: "parameters invalid" },
    )

    expect(result[0].request_projection.reasoning.high.set["reasoning.enabled"]).toBe(true)
    expect(result[1].capabilities.model_parameters[0]?.id).toBe("temperature")
    expect(result[2]).toBe(untouched)
  })

  it("rejects an invalid draft before any service write", () => {
    const model = modelFixture("invalid")

    expect(() => applyAdvancedModelTextDrafts(
      [model],
      {
        invalid: {
          projectionText: "[]",
          parameterText: "{}",
        },
      },
      { projectionInvalid: "projection invalid", parametersInvalid: "parameters invalid" },
    )).toThrow("projection invalid")
  })
})
