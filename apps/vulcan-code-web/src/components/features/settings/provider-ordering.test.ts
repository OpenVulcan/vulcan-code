import { describe, expect, it } from "vitest"

import {
  isFunctionalProvider,
  providerPriority,
} from "@/components/features/settings/provider-ordering"
import type { PublicAiProviderDescriptor } from "@/types"

// providerFixture creates the smallest complete ordering identity used by these tests.
// providerFixture 创建这些测试使用的最小完整排序身份。
function providerFixture(
  id: string,
  kind: PublicAiProviderDescriptor["kind"] = "system",
  entryKind: PublicAiProviderDescriptor["service_entries"][number]["kind"] = "conversation",
): PublicAiProviderDescriptor {
  return {
    id,
    display_name: id,
    kind,
    category: "official",
    credential_directory_visibility: "configured_only",
    credential_registration: "direct",
    service_entries: [{ id: `${id}-entry`, kind: entryKind }] as PublicAiProviderDescriptor["service_entries"],
  }
}

describe("provider ordering", () => {
  it("uses zero as the missing priority", () => {
    expect(providerPriority({}, "missing")).toBe(0)
    expect(providerPriority({ high: 20 }, "high")).toBe(20)
  })

  it("classifies only tool-only system providers as functional", () => {
    expect(isFunctionalProvider(providerFixture("tavily", "system", "provider_tools"))).toBe(true)
    expect(isFunctionalProvider(providerFixture("custom-tool", "custom", "provider_tools"))).toBe(false)
    expect(isFunctionalProvider(providerFixture("model"))).toBe(false)
  })
})
