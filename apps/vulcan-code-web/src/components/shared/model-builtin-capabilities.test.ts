import { describe, expect, it } from "vitest"

import { modelBuiltinCapabilities } from "@/components/shared/model-builtin-capabilities"

describe("modelBuiltinCapabilities", () => {
  // Verifies only the four requested route-native capabilities are displayed in stable order.
  // 验证仅以稳定顺序展示要求的四项路由原生能力。
  it("derives understanding and search without exposing unrelated native tools", () => {
    expect(modelBuiltinCapabilities({
      inputResources: ["audio", "file", "video", "image"],
      providerNativeTools: ["image_generation", "web_search"],
    })).toEqual([
      "image_understanding",
      "video_understanding",
      "audio_understanding",
      "web_search",
    ])
  })

  // Verifies absent declarations never create optimistic capability icons.
  // 验证缺失声明时绝不乐观生成能力图标。
  it("fails closed when the exact route declares no built-in capability", () => {
    expect(modelBuiltinCapabilities({})).toEqual([])
  })
})
