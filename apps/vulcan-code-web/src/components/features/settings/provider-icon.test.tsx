import { describe, expect, it } from "vitest"

import { resolveProviderIconKey } from "@/components/features/settings/provider-icon"

// Provider artwork resolution preserves family, product, theme, and fallback behavior.
// 供应商图标解析保留系列、产品、主题与回退行为。
describe("provider icon library", () => {
  // Built-in providers resolve their exact family artwork.
  // 内置供应商解析到各自精确的系列图标。
  it("resolves every implemented provider family", () => {
    expect(resolveProviderIconKey({ providerId: "minimax" })).toBe("minimax")
    expect(resolveProviderIconKey({ providerId: "openrouter" })).toBe("openrouter")
    expect(resolveProviderIconKey({ providerId: "openai" })).toBe("compatibility")
    expect(resolveProviderIconKey({ providerId: "anthropic" })).toBe("claude")
    expect(resolveProviderIconKey({ providerId: "google" })).toBe("gemini")
  })

  // Product artwork overrides its provider-family default.
  // 产品图标覆盖其供应商系列默认图标。
  it("prefers exact service-entry artwork", () => {
    expect(resolveProviderIconKey({
      providerId: "openai",
      serviceEntryId: "codex-subscription",
    })).toBe("codex")
  })

  // Unknown custom providers use the reviewed compatibility artwork.
  // 未知自定义供应商使用已审核的兼容图标。
  it("falls back to compatibility artwork", () => {
    expect(resolveProviderIconKey({ providerId: "custom:example" })).toBe("compatibility")
  })
})
