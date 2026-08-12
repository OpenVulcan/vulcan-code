import { describe, expect, it } from "vitest"

import type { ProviderUsageDailySummary } from "@/types"

import { providerUsageSummaryMetrics } from "./provider-usage-settings"

// dailyUsageRow creates one exact daily aggregate for display-adapter tests.
// dailyUsageRow 为展示适配器测试创建一条精确每日聚合。
function dailyUsageRow(overrides: Partial<ProviderUsageDailySummary> = {}): ProviderUsageDailySummary {
  return {
    calendarDate: "2026-08-12",
    providerId: "openai",
    serviceEntryId: "responses",
    consumerId: "workbench",
    operation: "chat",
    modelId: "gpt-5.6-terra",
    currency: "USD",
    attemptCount: 10,
    successfulAttemptCount: 9,
    reportedUsageAttemptCount: 8,
    totalTokens: 1_250,
    aggregateRevision: 1,
    ...overrides,
  }
}

describe("providerUsageSummaryMetrics", () => {
  // Verifies exact rows become stable product-component view models.
  // 验证精确计量行会转换为稳定的产品组件视图模型。
  it("aggregates attempts, coverage, and known tokens", () => {
    const metrics = providerUsageSummaryMetrics([
      dailyUsageRow(),
      dailyUsageRow({ attemptCount: 5, reportedUsageAttemptCount: 4, totalTokens: null }),
    ], true)

    expect(metrics.map((metric) => metric.id)).toEqual([
      "physical-attempts",
      "usage-coverage",
      "total-tokens",
    ])
    expect(metrics.map((metric) => metric.value)).toEqual(["15", "80.0%", "1,250"])
  })

  // Verifies absent attempts remain visibly unknown instead of inventing a percentage.
  // 验证没有调用事实时保持明确未知，而不是虚构百分比。
  it("keeps empty coverage unknown", () => {
    const metrics = providerUsageSummaryMetrics([], false)

    expect(metrics[1].value).toBe("—")
    expect(metrics[1].label).toBe("Usage coverage")
  })
})
