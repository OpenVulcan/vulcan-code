import { describe, expect, it } from "vitest"

import type { TranslationKey } from "@/i18n"
import type { PublicAiAccountUsageWindow } from "@/types"

import {
  formatCurrencyAmount,
  kimiUsageWindowLabel,
  usageCapabilitySupportsQuotaWindows,
} from "./credential-usage-overview"

// kimiUsageWindow creates one minimal normalized Kimi quota window for label tests.
// kimiUsageWindow 为名称测试创建一个最小规范化 Kimi 额度窗口。
function kimiUsageWindow(id: string, displayName = id): PublicAiAccountUsageWindow {
  return {
    id,
    display_name: displayName,
    unit: "provider_defined",
    unlimited: false,
    available: true,
  }
}

// testTranslation resolves only the Kimi usage strings exercised by this module.
// testTranslation 仅解析本模块验证的 Kimi 用量文案。
function testTranslation(key: TranslationKey): string {
  const messages: Partial<Record<TranslationKey, string>> = {
    "composer.accountCard.kimiWeeklyUsage": "周用量",
    "composer.accountCard.kimiFiveHourUsage": "5 小时用量",
    "composer.accountCard.kimiLimit": "额度",
  }
  return messages[key] ?? key
}

describe("formatCurrencyAmount", () => {
  // Verifies provider decimal strings are rounded without floating-point conversion.
  // 验证供应商十进制字符串在不经过浮点转换的情况下完成舍入。
  it("renders exactly two decimal places", () => {
    expect(formatCurrencyAmount("0.596171815")).toBe("0.60")
    expect(formatCurrencyAmount("12")).toBe("12.00")
    expect(formatCurrencyAmount("12.3")).toBe("12.30")
    expect(formatCurrencyAmount("999.999")).toBe("1000.00")
  })

  // Verifies unsupported provider representations remain visible instead of being corrupted.
  // 验证不支持的供应商表示形式保持可见而不会被破坏。
  it("preserves unsupported representations", () => {
    expect(formatCurrencyAmount("unlimited")).toBe("unlimited")
  })
})

describe("usageCapabilitySupportsQuotaWindows", () => {
  // Verifies only backend contracts with real plan windows retain the composer quota section.
  // 验证只有提供真实套餐窗口的后端契约才保留会话额度区域。
  it("separates quota windows from balance-only and unsupported usage", () => {
    expect(usageCapabilitySupportsQuotaWindows("mini_max_token_plan")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("tavily_credits")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("kimi_coding_plan")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("open_router_credits")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("deep_seek_balance")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("unsupported")).toBe(false)
  })
})

describe("kimiUsageWindowLabel", () => {
  // Verifies ModelCore's stable Kimi metrics are localized instead of displayed as raw identifiers.
  // 验证 ModelCore 的稳定 Kimi 指标会被本地化，而不是显示为原始标识。
  it("localizes known and fallback Kimi usage metrics", () => {
    expect(kimiUsageWindowLabel(kimiUsageWindow("weekly_usage"), testTranslation)).toBe("周用量")
    expect(kimiUsageWindowLabel(kimiUsageWindow("five_hour_usage"), testTranslation)).toBe("5 小时用量")
    expect(kimiUsageWindowLabel(kimiUsageWindow("5_hour_usage"), testTranslation)).toBe("5 小时用量")
    expect(kimiUsageWindowLabel(kimiUsageWindow("limit_1"), testTranslation)).toBe("额度 1")
  })

  // Verifies provider-owned named metrics remain intact when no product translation exists.
  // 验证不存在产品翻译时，供应商拥有的具名指标保持不变。
  it("preserves provider-owned metric names", () => {
    expect(
      kimiUsageWindowLabel(
        kimiUsageWindow("custom_provider_pool", "Custom provider pool"),
        testTranslation,
      ),
    ).toBe("Custom provider pool")
  })
})
