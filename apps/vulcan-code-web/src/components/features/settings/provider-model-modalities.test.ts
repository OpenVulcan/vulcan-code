import { describe, expect, it } from "vitest"

import { providerModelModalities } from "@/components/features/settings/provider-model-modalities"
import type { PublicAiModel } from "@/types"

// modelFixture builds the minimum exact catalog model needed by modality tests.
// modelFixture 构建模态测试所需的最小精确目录模型。
function modelFixture(overrides: Partial<PublicAiModel["capabilities"]>): PublicAiModel {
  return {
    id: "model",
    display_name: "Model",
    capabilities: {
      evidence_revision: "test",
      reasoning: "unsupported",
      prompt_cache: {
        provider_default_behavior: "provider_default",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
      ...overrides,
    },
  }
}

describe("providerModelModalities", () => {
  it("shows exact text, image, and video inputs for a multimodal conversation route", () => {
    const modalities = providerModelModalities(modelFixture({
      operations: ["conversation_respond"],
      input_resources: ["image", "video"],
    }))

    expect(modalities).toEqual({
      input: ["text", "image", "video"],
      output: ["text"],
    })
  })

  it("shows specialized embedding and rerank outputs without inventing media outputs", () => {
    const embedding = providerModelModalities(modelFixture({
      operations: ["embedding_create"],
      input_resources: ["image"],
      embedding: {
        tasks: [],
        output_kinds: [],
        encodings: [],
        dimensions: [],
        normalization: "unknown",
        distance_metric: "unknown",
      },
    }))
    const rerank = providerModelModalities(modelFixture({
      operations: ["rerank_documents"],
      rerank: {
        truncation: [],
        supports_return_content: false,
        score_semantics: "unknown",
      },
    }))

    expect(embedding).toEqual({
      input: ["text", "image"],
      output: ["embedding"],
    })
    expect(rerank).toEqual({
      input: ["text"],
      output: ["rerank"],
    })
  })
})
