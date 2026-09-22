import { describe, expect, it } from "vitest"

import {
  directCustomModelSettingsDraft,
  directProviderModelSettingsEqual,
  projectOpenAiReasoningEfforts,
  resolveDirectCustomProviderDefaults,
  systemProviderCapabilitySettingsDraft,
  systemProviderEditableOverrides,
} from "@/components/features/settings/direct-provider-model-settings"
import type { CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import { zhCNTranslations } from "@/i18n-messages.zh-CN"
import { enUSTranslations } from "@/i18n-messages.en-US"
import type { TranslationKey } from "@/i18n-messages"

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
      output_limit: 4_000,
      input_resources: [],
      structured_output: "json_object",
      reasoning: "unsupported",
      reasoning_control: "fixed_off",
      reasoning_efforts: ["none"],
    }))

    expect(draft).toEqual({
      context_limit: 32_000,
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

  // System providers retain immutable Manifest facts internally while exposing only authorized limit overrides.
  // 系统供应商仅在内部保留不可变 Manifest 事实，界面只暴露获授权的限制覆盖。
  it("keeps system provider capabilities immutable and preserves adaptive levels", () => {
    const model = rowWithCapabilities().effectiveModel!
    model.capabilities.input_resources = ["image", "video"]
    model.capabilities.structured_output = "json_schema"
    model.capabilities.reasoning = "native"
    model.capabilities.reasoning_control = "provider_adaptive"
    model.capabilities.reasoning_efforts = ["adaptive"]

    expect(systemProviderCapabilitySettingsDraft(model, {
      context_limit: 128_000,
      output_limit: 16_000,
      input_resources: ["audio", "file"],
      structured_output: "unsupported",
      reasoning: "unsupported",
      reasoning_control: "fixed_off",
      reasoning_efforts: ["none", "high"],
      request_projection: {
        reasoning: {},
        model_parameter_paths: {},
        defaults: { thinking: "disabled" },
        overrides: {},
        delete: [],
      },
    })).toEqual({
      context_limit: 128_000,
      output_limit: 16_000,
      input_resources: ["image", "video"],
      structured_output: "json_schema",
      reasoning: "native",
      reasoning_control: "provider_adaptive",
      reasoning_efforts: ["adaptive"],
    })
  })

  // The browser must never resend stale forbidden system-provider fields with an allowed limit edit.
  // 浏览器绝不能在保存获准限制时重新发送陈旧的系统供应商禁用字段。
  it("strips forbidden system provider capability overrides", () => {
    expect(systemProviderEditableOverrides({
      context_limit: 128_000,
      output_limit: 16_000,
      input_resources: ["image"],
      structured_output: "json_schema",
      reasoning: "native",
      reasoning_control: "toggle",
      reasoning_efforts: ["none", "adaptive"],
      request_projection: {
        reasoning: {},
        model_parameter_paths: {},
        defaults: {},
        overrides: {},
        delete: [],
      },
    })).toEqual({
      context_limit: 128_000,
      output_limit: 16_000,
    })
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

    expect(keys.map((key) => zhCNTranslations[key])).toEqual([
      "关闭（none）",
      "最小（minimal）",
      "低（low）",
      "中（medium）",
      "高（high）",
      "超高（xhigh）",
      "最大（max）",
    ])
    expect(keys.map((key) => enUSTranslations[key])).toEqual([
      "Off",
      "Minimal",
      "Low",
      "Medium",
      "High",
      "Extra high",
      "Maximum",
    ])
    expect(zhCNTranslations["settings.customModels.effortAdaptive"]).toBe("自动（adaptive）")
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
