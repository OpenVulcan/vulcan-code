import { describe, expect, it } from "vitest"

import {
  formatCurrencyAmount,
  usageCapabilitySupportsQuotaWindows,
} from "./credential-usage-overview"

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
