import { describe, expect, it } from "vitest"

import type { TranslationKey } from "@/i18n"
import type { PublicAiAccountUsageWindow, PublicAiCredentialProfile } from "@/types"

import {
  commandCodeUsageRows,
  commandCodeUsageWindowLabel,
  formatCurrencyAmount,
  credentialUsageIsNonPlanAccount,
  kimiUsageWindowLabel,
  usageWindowValue,
  usageCapabilityRequiresManagementKey,
  usageCapabilitySupportsQuotaWindows,
} from "./credential-usage-overview"

// credentialProfile creates one minimal manifest-projected credential profile for semantics tests.
// credentialProfile 为语义测试创建一个最小 Manifest 投影凭据配置档。
function credentialProfile(
  accountUsageQuery: PublicAiCredentialProfile["account_usage_query"],
): PublicAiCredentialProfile {
  return {
    id: "test-profile",
    display_name: "Test profile",
    credential_mode: "token",
    billing_kind: "pay_as_you_go",
    account_usage_query: accountUsageQuery,
  }
}

describe("credentialUsageIsNonPlanAccount", () => {
  // Verifies UI semantics follow the credential profile and never a provider identifier.
  // 验证 UI 语义跟随凭据配置档而绝不跟随供应商标识。
  it("uses the manifest-projected usage capability", () => {
    expect(credentialUsageIsNonPlanAccount(credentialProfile("deep_seek_balance"))).toBe(true)
    expect(credentialUsageIsNonPlanAccount(credentialProfile("open_router_credits"))).toBe(false)
    expect(credentialUsageIsNonPlanAccount(undefined)).toBe(false)
  })
})

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

// testTranslation resolves the provider usage strings exercised by this module.
// testTranslation 解析本模块验证的供应商用量文案。
function testTranslation(key: TranslationKey): string {
  const messages: Partial<Record<TranslationKey, string>> = {
    "composer.accountCard.kimiWeeklyUsage": "周用量",
    "composer.accountCard.kimiFiveHourUsage": "5 小时用量",
    "composer.accountCard.kimiLimit": "额度",
    "composer.accountCard.commandCodeMonthlyCredits": "每月额度",
    "composer.accountCard.commandCodePurchasedCredits": "购买额度",
    "composer.accountCard.commandCodeFreeCredits": "赠送额度",
    "composer.accountCard.commandCodeFiveHourRequests": "5 小时请求",
    "composer.accountCard.commandCodeWeeklyRequests": "每周请求",
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
    expect(usageCapabilitySupportsQuotaWindows("command_code_usage")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("open_router_credits")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("deep_seek_balance")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("unsupported")).toBe(false)
  })
})

describe("usageCapabilityRequiresManagementKey", () => {
  // Verifies management-key presentation follows the manifest-selected host primitive.
  // 验证管理密钥展示跟随清单选择的宿主原语。
  it("separates management credentials from inference credentials", () => {
    expect(usageCapabilityRequiresManagementKey("open_router_credits")).toBe(true)
    expect(usageCapabilityRequiresManagementKey("mini_max_token_plan")).toBe(false)
    expect(usageCapabilityRequiresManagementKey("unsupported")).toBe(false)
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

describe("CommandCode usage presentation", () => {
  // Verifies stable credit windows are separated from provider response ordering for the compact layout.
  // 验证稳定额度窗口与供应商响应顺序分离，以支持紧凑布局。
  it("groups monthly, purchased, and free credits into fixed display positions", () => {
    const rows = commandCodeUsageRows([
      kimiUsageWindow("free-credits"),
      kimiUsageWindow("weekly-requests"),
      kimiUsageWindow("monthly-credits"),
      kimiUsageWindow("purchased-credits"),
    ])

    expect(rows?.monthly?.id).toBe("monthly-credits")
    expect(rows?.purchased?.id).toBe("purchased-credits")
    expect(rows?.free?.id).toBe("free-credits")
  })

  // Verifies unsupported or request-only windows do not create an invented credit row.
  // 验证不支持或仅请求类窗口不会创建虚构的额度行。
  it("returns no credit layout when no credit windows are present", () => {
    expect(commandCodeUsageRows([kimiUsageWindow("weekly-requests")])).toBeNull()
  })

  // Verifies stable credit-window identifiers are localized independently from provider wire text.
  // 验证稳定额度窗口标识独立于供应商 Wire 文案完成本地化。
  it("localizes known credit and request windows", () => {
    expect(commandCodeUsageWindowLabel(kimiUsageWindow("monthly-credits"), testTranslation)).toBe("每月额度")
    expect(commandCodeUsageWindowLabel(kimiUsageWindow("purchased-credits"), testTranslation)).toBe("购买额度")
    expect(commandCodeUsageWindowLabel(kimiUsageWindow("free-credits"), testTranslation)).toBe("赠送额度")
    expect(commandCodeUsageWindowLabel(kimiUsageWindow("five-hour-requests"), testTranslation)).toBe("5 小时请求")
    expect(commandCodeUsageWindowLabel(kimiUsageWindow("weekly-requests"), testTranslation)).toBe("每周请求")
  })

  // Verifies monetary quota windows use the same bounded decimal presentation as account balance.
  // 验证货币额度窗口使用与账号余额一致的有界小数展示。
  it("formats remaining and total credits as currency", () => {
    expect(usageWindowValue({
      ...kimiUsageWindow("monthly-credits"),
      remaining: "69.761368264",
      total: "70",
      unit: "USD",
    }, testTranslation)).toBe("69.76 / 70.00 USD")
  })
})
