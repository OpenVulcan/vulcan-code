import { describe, expect, it } from "vitest"

import type { TranslationKey } from "@/i18n"
import type { PublicAiAccountUsageWindow, PublicAiCredentialProfile } from "@/types"

import {
  codexUsageWindowLabel,
  commandCodeUsageRows,
  commandCodeUsageWindowLabel,
  formatCurrencyAmount,
  credentialUsageIsNonPlanAccount,
  kimiUsageWindowLabel,
  openDesignUsageWindowLabel,
  usageWindowValue,
  usageWindowPercentage,
} from "./credential-usage-overview"
import {
  usageCapabilityRequiresManagementKey,
  usageCapabilitySupportsQuotaWindows,
} from "./credential-usage-capability"

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
    expect(credentialUsageIsNonPlanAccount(credentialProfile("node_ai_balance"))).toBe(true)
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

describe("unknown quota values", () => {
  it("distinguishes missing remaining quota from an exhausted quota", () => {
    // Both windows share one declared allowance; only the second reports an exact zero.
    // 两个窗口具有相同的声明额度，仅第二个报告精确零值。
    const unknown = { ...kimiUsageWindow("quota"), total: "100" };
    const exhausted = { ...unknown, remaining: "0" };
    expect(usageWindowPercentage(unknown)).toBeNull();
    expect(usageWindowPercentage(exhausted)).toBe(0);
    expect(usageWindowValue(unknown, testTranslation)).toContain("-");
    expect(usageWindowValue(exhausted, testTranslation)).toContain("0 / 100");
  });
});

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
    expect(usageCapabilitySupportsQuotaWindows("open_design_wallet")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("opencode_go")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("open_ai_codex_usage")).toBe(true)
    expect(usageCapabilitySupportsQuotaWindows("open_router_credits")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("deep_seek_balance")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("node_ai_balance")).toBe(false)
    expect(usageCapabilityRequiresManagementKey("node_ai_balance")).toBe(false)
    expect(usageCapabilitySupportsQuotaWindows("unsupported")).toBe(false)
  })
})

describe("OpenDesign plan presentation", () => {
  // Period labels follow explicit metadata rather than policy names or mutable array positions.
  // 周期标签跟随显式元数据，而不是策略名称或可变数组位置。
  it("localizes provider durations and rejects missing duration metadata", () => {
    // Translation keys identify formatting behavior without freezing localized wording.
    // 目录键标识格式行为，不锁定本地化文案。
    const translation = (key: TranslationKey) => `${key} {count}`
    for (const [seconds, key, count] of [
      [18000, "composer.accountCard.openDesignHours", 5],
      [604800, "composer.accountCard.openDesignDays", 7],
      [75, "composer.accountCard.openDesignSeconds", 75],
    ] as const) {
      expect(openDesignUsageWindowLabel({ ...kimiUsageWindow("opaque-policy"), duration_seconds: seconds }, translation))
        .toBe(`${key} ${count}`)
    }
    expect(openDesignUsageWindowLabel(kimiUsageWindow("opaque-policy"), (key) => key))
      .toBe("composer.accountCard.usageInvalid")
  })

  // The percentage must stay finite even when a credit pool exceeds the entire Number range.
  // 即使积分池超出 Number 的整个范围，百分比也必须保持有限值。
  it("divides arbitrary-size integer quotas before converting to a percentage", () => {
    // Synthetic counters exceed floating-point range while preserving an exact half-full ratio.
    // 合成计数超出浮点数范围，同时保留精确的一半剩余比例。
    const window = {
      ...kimiUsageWindow("huge-policy"),
      remaining: `5${"0".repeat(399)}`,
      total: `1${"0".repeat(400)}`,
    }
    expect(usageWindowPercentage(window)).toBe(50)
    expect(usageWindowPercentage({ ...window, remaining: "0" })).toBe(0)
    expect(usageWindowPercentage({ ...window, remaining: window.total })).toBe(100)
    expect(usageWindowPercentage({ ...window, available: false })).toBeNull()
  })
})

describe("codexUsageWindowLabel", () => {
  // Period labels follow the reported duration, and model buckets keep their provider name as a prefix.
  // 周期标签跟随报告的时长，模型限额桶保留供应商名称作为前缀。
  it("derives the period from duration metadata and prefixes bucket names", () => {
    // Translation keys identify formatting behavior without freezing localized wording.
    // 目录键标识格式行为，不锁定本地化文案。
    const translation = (key: TranslationKey) => `${key} {count}`
    const window = kimiUsageWindow("codex:primary")
    expect(codexUsageWindowLabel({ ...window, duration_seconds: 18000 }, translation))
      .toBe("composer.accountCard.codexHoursWindow 5")
    expect(codexUsageWindowLabel({ ...window, duration_seconds: 604800 }, translation))
      .toBe("composer.accountCard.codexWeeklyWindow {count}")
    expect(codexUsageWindowLabel({ ...window, duration_seconds: 172800 }, translation))
      .toBe("composer.accountCard.codexDaysWindow 2")
    expect(codexUsageWindowLabel({ ...window, duration_seconds: 604800, bucket_name: "Bucket" }, translation))
      .toBe("Bucket · composer.accountCard.codexWeeklyWindow {count}")
    expect(codexUsageWindowLabel({ ...window, display_name: "Provider" }, translation)).toBe("Provider")
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
