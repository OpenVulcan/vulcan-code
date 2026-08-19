import { describe, expect, it } from "vitest"

import {
  isFunctionalProvider,
  orderCredentialDirectoryProviders,
  orderProvidersByPriority,
  providerPriority,
} from "@/components/features/settings/provider-ordering"
import type { PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"

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
  it("uses zero as the missing priority and sorts explicit values high to low", () => {
    // providers keeps the original order available as the final tie-break.
    // providers 保留原始顺序，作为最终同级排序依据。
    const providers = [providerFixture("zero"), providerFixture("high"), providerFixture("low")]
    expect(providerPriority({}, "missing")).toBe(0)
    expect(orderProvidersByPriority(providers, { high: 20, low: -5 }).map((provider) => provider.id))
      .toEqual(["high", "zero", "low"])
  })

  it("classifies only tool-only system providers as functional", () => {
    expect(isFunctionalProvider(providerFixture("tavily", "system", "provider_tools"))).toBe(true)
    expect(isFunctionalProvider(providerFixture("custom-tool", "custom", "provider_tools"))).toBe(false)
    expect(isFunctionalProvider(providerFixture("model"))).toBe(false)
  })

  it("orders credential providers by configured models before provider priority", () => {
    // providers contains one configured model provider and two providers without model routes.
    // providers 包含一个已配置模型供应商和两个无模型路由供应商。
    const providers = [
      providerFixture("unconfigured-high"),
      providerFixture("configured-low"),
      providerFixture("unconfigured-zero"),
    ]
    // snapshot supplies the exact executable route used to define configured models.
    // snapshot 提供用于定义已配置模型的精确可执行路由。
    const snapshot = {
      providers,
      providerPriorities: {
        "unconfigured-high": 50,
        "configured-low": -10,
      },
      modelRouteRecords: [{
        providerId: "configured-low",
        connectionId: "configured-low-connection",
        serviceEntryId: "configured-low-entry",
        modelId: "configured-model",
        availableProtocols: ["openai-chat-completions.v1"],
        manifestRevision: 1,
        executable: true,
      }],
    } satisfies Pick<PublicAiSettingsSnapshot, "providers" | "providerPriorities" | "modelRouteRecords">
    expect(orderCredentialDirectoryProviders(snapshot).map((provider) => provider.id))
      .toEqual(["configured-low", "unconfigured-high", "unconfigured-zero"])
  })
})
