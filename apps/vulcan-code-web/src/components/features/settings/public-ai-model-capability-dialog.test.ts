import { describe, expect, it } from "vitest"

import {
  buildCapabilityOverridePatch,
  normalizeRequestProjection,
} from "@/components/features/settings/public-ai-model-capability-dialog"
import type { PublicAiModelCapabilityOverrides } from "@/types"

// currentSettingsFixture represents one complete effective model layer.
// currentSettingsFixture 表示一份完整的有效模型层。
const currentSettingsFixture: PublicAiModelCapabilityOverrides = {
  context_limit: 16_384,
  output_limit: 4_096,
  input_resources: ["image"],
  reasoning: "native",
  reasoning_control: "effort_selection",
  reasoning_efforts: ["none", "low"],
  tool_calling: "native",
  request_projection: {
    reasoning: {},
    model_parameter_paths: {},
    defaults: { "thinking.type": "enabled" },
    overrides: {},
    delete: [],
  },
}

describe("buildCapabilityOverridePatch", () => {
  it("persists only a touched model field so other fields keep inheriting", () => {
    expect(buildCapabilityOverridePatch(
      currentSettingsFixture,
      undefined,
      new Set(["context_limit"]),
    )).toEqual({ context_limit: 16_384 })
  })

  it("persists the complete atomic reasoning contract when one reasoning control changes", () => {
    expect(buildCapabilityOverridePatch(
      currentSettingsFixture,
      undefined,
      new Set(["reasoning_efforts"]),
    )).toEqual({
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["none", "low"],
    })
  })

  it("retains existing sparse fields while adding an explicit empty collection", () => {
    expect(buildCapabilityOverridePatch(
      currentSettingsFixture,
      { output_limit: 2_048 },
      new Set(["input_resources"]),
    )).toEqual({
      output_limit: 2_048,
      input_resources: ["image"],
    })
  })
})

describe("normalizeRequestProjection", () => {
  it("repairs legacy reasoning mutations that omit the delete collection", () => {
    expect(normalizeRequestProjection({
      reasoning: {
        low: { set: { "thinking.type": "enabled" } } as { set: Record<string, unknown> },
      },
    } as never)).toEqual({
      reasoning: {
        low: { set: { "thinking.type": "enabled" }, delete: [] },
      },
      model_parameter_paths: {},
      defaults: {},
      overrides: {},
      delete: [],
    })
  })
})
