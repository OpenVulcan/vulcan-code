import { describe, expect, it } from "vitest"

import type { TranslationKey } from "@/i18n-messages"
import type { PublicAiServiceEntry } from "@/types"
import {
  localizedProviderDescription,
  localizedServiceEntryPresentation,
} from "./provider-presentation"

// translationFixture exposes stable keys so presentation composition can be asserted directly.
// translationFixture 直接暴露稳定翻译键，以便断言展示组合。
const translationFixture = (key: TranslationKey) => key

// entryFixture creates the minimum complete service entry required by presentation tests.
// entryFixture 创建展示测试所需的最小完整服务入口。
function entryFixture(id: string, displayName: string): PublicAiServiceEntry {
  return {
    id,
    display_name: displayName,
    kind: "conversation",
    base_url: "https://example.com",
    credential_profiles: [],
    authentication: "bearer_authorization",
    protocol_routes: [],
  }
}

describe("localizedServiceEntryPresentation", () => {
  // Verifies Alibaba Token Plan entries render independent product, account, and region labels.
  // 验证阿里云 Token Plan 入口独立渲染产品、账号范围与区域标签。
  it("composes multiple Alibaba classification tags", () => {
    expect(localizedServiceEntryPresentation(
      "alibaba",
      entryFixture("token-plan-personal-global", "Token Plan Personal International"),
      translationFixture,
    )).toEqual({
      title: "settings.serviceEntry.title.tokenPlan",
      tags: [
        "settings.serviceEntry.tag.personal",
        "settings.serviceEntry.tag.international",
      ],
    })
  })

  // Verifies providers without an explicit split retain their authoritative entry title.
  // 验证没有显式拆分规则的供应商保留其权威入口标题。
  it("preserves non-Alibaba entry names", () => {
    expect(localizedServiceEntryPresentation(
      "openrouter",
      entryFixture("public", "Public API"),
      translationFixture,
    )).toEqual({ title: "Public API", tags: [] })
  })
})

describe("localizedProviderDescription", () => {
  // Verifies built-in and unknown providers use explicit description keys.
  // 验证内置与未知供应商分别使用明确的介绍键。
  it("resolves registered and fallback descriptions", () => {
    expect(localizedProviderDescription("alibaba", translationFixture))
      .toBe("settings.providerDescription.alibaba")
    expect(localizedProviderDescription("future-provider", translationFixture))
      .toBe("settings.providerDescription.default")
  })
})
