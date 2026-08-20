import { describe, expect, it } from "vitest"

import {
  directCustomModelSettingsDraft,
  directProviderModelSettingsEqual,
  projectOpenAiReasoningEfforts,
  resolveDirectCustomProviderDefaults,
} from "@/components/features/settings/direct-provider-model-settings"
import type { CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import { translations, type TranslationKey } from "@/i18n-messages"

// rowWithCapabilities creates the exact minimal merged row required by direct settings projection tests.
// rowWithCapabilities 创建直接设置投影测试所需的精确最小合并行。
function rowWithCapabilities(
  capabilityOverrides?: CustomProviderModelRow["capabilityOverrides"],
): CustomProviderModelRow {
  return {
    id: "model-a",
    displayName: "Model A",
    accounts: [],
    capabilityOverrides,
    effectiveModel: {
      id: "model-a",
      display_name: "Model A",
      capabilities: {
        operations: ["conversation_respond"],
        context_limit: 64_000,
        output_limit: 8_000,
        input_resources: ["image"],
        output_resources: ["image"],
        reasoning: "unknown",
        reasoning_control: "unknown",
        reasoning_efforts: [],
        prompt_cache: {
          provider_default_behavior: "Provider default",
          supports_managed_explicit: false,
          supported_ttl_options: [],
        },
        model_parameters: [],
        evidence_revision: "test",
      },
    },
  }
}

describe("direct provider model settings", () => {
  it("defaults reasoning on without exposing output overrides", () => {
    const draft = directCustomModelSettingsDraft(rowWithCapabilities())

    expect(draft).toEqual({
      context_limit: 64_000,
      context_window_policy: undefined,
      output_limit: 8_000,
      input_resources: ["image"],
      structured_output: "json_object",
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    })
    expect("output_resources" in draft).toBe(false)
  })

  it("keeps explicit disablement and complete model-level values", () => {
    const draft = directCustomModelSettingsDraft(rowWithCapabilities({
      context_limit: 32_000,
      context_window_policy: undefined,
      output_limit: 4_000,
      input_resources: [],
      structured_output: "json_object",
      reasoning: "unsupported",
      reasoning_control: "fixed_off",
      reasoning_efforts: ["none"],
    }))

    expect(draft).toEqual({
      context_limit: 32_000,
      context_window_policy: undefined,
      output_limit: 4_000,
      input_resources: [],
      structured_output: "json_object",
      reasoning: "unsupported",
      reasoning_control: "fixed_off",
      reasoning_efforts: ["none"],
    })
  })

  it("projects provider-specific levels onto the standard OpenAI set", () => {
    expect(projectOpenAiReasoningEfforts(["adaptive", "ultra"])).toEqual([
      "none",
      "minimal",
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
    ])
    expect(projectOpenAiReasoningEfforts(["high", "low"])).toEqual(["low", "high"])
    expect(projectOpenAiReasoningEfforts(["none", "high"])).toEqual(["none", "high"])
  })

  it("preserves a reasoning model that cannot be turned off", () => {
    const draft = directCustomModelSettingsDraft(rowWithCapabilities({
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["high"],
    }))

    expect(draft.reasoning_efforts).toEqual(["high"])
  })

  // This regression guard keeps canonical effort tokens visible only as annotations in Chinese.
  // 此回归护栏确保规范档位值仅在中文界面作为括号注释显示。
  it("annotates Chinese reasoning labels without duplicating English labels", () => {
    const keys: TranslationKey[] = [
      "settings.customModels.effortNone",
      "settings.customModels.effortMinimal",
      "settings.customModels.effortLow",
      "settings.customModels.effortMedium",
      "settings.customModels.effortHigh",
      "settings.customModels.effortExtraHigh",
      "settings.customModels.effortMax",
    ]

    expect(keys.map((key) => translations["zh-CN"][key])).toEqual([
      "关闭（none）",
      "最小（minimal）",
      "低（low）",
      "中（medium）",
      "高（high）",
      "超高（xhigh）",
      "最大（max）",
    ])
    expect(keys.map((key) => translations["en-US"][key])).toEqual([
      "Off",
      "Minimal",
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Maximum",
    ])
  })

  it("treats collection order as equal while retaining every owned scalar difference", () => {
    expect(directProviderModelSettingsEqual(
      {
        context_limit: 64_000,
        output_limit: 8_000,
        input_resources: ["image", "file"],
        reasoning: "native",
        reasoning_control: "effort_selection",
        reasoning_efforts: ["none", "medium", "high"],
      },
      {
        context_limit: 64_000,
        output_limit: 8_000,
        input_resources: ["file", "image"],
        reasoning: "native",
        reasoning_control: "effort_selection",
        reasoning_efforts: ["high", "none", "medium"],
      },
    )).toBe(true)
    expect(directProviderModelSettingsEqual(
      { context_limit: 64_000 },
      { context_limit: 32_000 },
    )).toBe(false)
  })

  it("replaces a removed provider layer with an isolated system-default draft", () => {
    const systemDefaults = {
      context_limit: 200_000,
      output_limit: 32_768,
      input_resources: [],
      reasoning: "native" as const,
      reasoning_control: "effort_selection" as const,
      reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    }
    const first = resolveDirectCustomProviderDefaults(systemDefaults, undefined)
    first.input_resources?.push("image")
    first.reasoning_efforts.splice(0)

    expect(resolveDirectCustomProviderDefaults(systemDefaults, undefined)).toEqual({
      context_limit: 200_000,
      output_limit: 32_768,
      input_resources: [],
      reasoning: "native",
      reasoning_control: "effort_selection",
      reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    })
  })
})
