import { describe, expect, it } from "vitest"

import { filterProviderModels } from "@/components/features/settings/provider-model-filter"

// models defines stable fixtures covering display-name, identifier, and enabled-state filtering.
// models 定义覆盖显示名称、模型标识与启用状态筛选的稳定夹具。
const models = [
  { modelId: "deepseek/deepseek-v4-pro", displayName: "DeepSeek V4 Pro", enabled: true },
  { modelId: "moonshotai/kimi-k3", displayName: "Kimi K3", enabled: false },
  { modelId: "qwen/qwen3.7", displayName: "Qwen 3.7", enabled: true },
]

describe("filterProviderModels", () => {
  it("matches display names and model identifiers case-insensitively", () => {
    expect(filterProviderModels(models, "DEEPSEEK V4", "all")).toEqual([models[0]])
    expect(filterProviderModels(models, "moonshotai/", "all")).toEqual([models[1]])
  })

  it("filters enabled and disabled models independently from search text", () => {
    expect(filterProviderModels(models, "", "enabled")).toEqual([models[0], models[2]])
    expect(filterProviderModels(models, "kimi", "disabled")).toEqual([models[1]])
    expect(filterProviderModels(models, "qwen", "disabled")).toEqual([])
  })

  it("trims search text and preserves provider catalog order", () => {
    expect(filterProviderModels(models, "  ", "all")).toEqual(models)
  })
})
