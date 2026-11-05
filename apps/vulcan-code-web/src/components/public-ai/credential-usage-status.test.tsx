// @vitest-environment jsdom

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type {
  PublicAiAccountUsage,
  PublicAiAccountUsageWindow,
  PublicAiConnection,
  PublicAiCredentialProfile,
} from "@/types"
import { CredentialUsageStatusLine } from "./credential-usage-status"

// Catalog keys stand in for translations so assertions never depend on localized prose.
// 目录键代替翻译，使断言不依赖本地化文案。
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }))

// Synthetic connection carries only the public metadata the status fragment receives.
// 合成连接只携带状态片段接收的公开元数据。
const connection: PublicAiConnection = {
  id: "synthetic-connection", provider_id: "synthetic", service_entry_id: "synthetic-entry",
  credential_profile_id: "synthetic-profile", billing_kind: "token_plan", usage_capability: "mini_max_token_plan",
  usage_requires_management_key: false, has_management_key: false, credential_mode: "token",
  credential_id: "synthetic-credential", display_name: "Synthetic account", enabled: true,
  pending_validation: false,
}

// profile declares the usage contract under test.
// profile 声明被测的用量契约。
function profile(query: PublicAiCredentialProfile["account_usage_query"]): PublicAiCredentialProfile {
  return {
    id: "synthetic-profile", display_name: "Synthetic profile", credential_mode: "token",
    billing_kind: "token_plan", account_usage_query: query,
  }
}

// countWindow builds one available count window with exact remaining and total counts.
// countWindow 构建一个带精确剩余数与总数的可用次数窗口。
function countWindow(id: string, remaining: number, total: number, bucket?: string): PublicAiAccountUsageWindow {
  return {
    id, display_name: id, bucket_name: bucket, remaining: String(remaining), total: String(total),
    unit: "requests", available: true, unlimited: false,
  }
}

// usage builds one supported observation from the supplied balance and windows.
// usage 根据给定余额与窗口构建一个受支持的观测。
function usage(overrides: Partial<PublicAiAccountUsage>): PublicAiAccountUsage {
  return { credential_id: "synthetic-credential", availability: "supported", quota_windows: [], ...overrides }
}

// render mounts the fragment as static markup and returns a detached root for DOM assertions.
// render 以静态标记挂载片段，返回独立根节点供 DOM 断言。
function render(query: PublicAiCredentialProfile["account_usage_query"], observation?: PublicAiAccountUsage): HTMLDivElement {
  const root = document.createElement("div")
  root.innerHTML = renderToStaticMarkup(
    <CredentialUsageStatusLine connection={connection} credentialProfile={profile(query)} usage={observation} />,
  )
  return root
}

describe("account status usage fragment", () => {
  it("shows only the balance for a balance account", () => {
    const root = render("deep_seek_balance", usage({ balance_amount: "12.5", currency: "CNY" }))
    expect(root.querySelector('[data-slot="account-status-balance"]')?.textContent).toContain("12.50 CNY")
    expect(root.querySelector('[data-slot="account-status-quota"]')).toBeNull()
    expect(root.querySelector('[role="progressbar"]')).toBeNull()
  })

  it("shows a gauge of the most constrained main window for a quota account", () => {
    const root = render("mini_max_token_plan", usage({
      plan_name: "Token Plan",
      quota_windows: [
        countWindow("general:current", 40, 100, "general"),
        countWindow("general:weekly", 90, 100, "general"),
        // A side bucket never stands in for the plan the chat model draws on.
        // 附属额度桶绝不代替对话模型所用的套餐。
        countWindow("video:current", 0, 10, "video"),
      ],
    }))
    const bars = root.querySelectorAll('[role="progressbar"]')
    expect(bars).toHaveLength(1)
    expect(bars[0].getAttribute("aria-valuenow")).toBe("40")
    expect(root.querySelector('[data-slot="account-status-quota"]')?.textContent).toContain("40%")
    expect(root.querySelector('[data-slot="account-status-balance"]')).toBeNull()
  })

  it("renders nothing when the account reports neither balance nor quota", () => {
    expect(render("mini_max_token_plan", usage({ plan_name: "Token Plan" })).innerHTML).toBe("")
    expect(render("mini_max_token_plan", undefined).innerHTML).toBe("")
    // A failed read keeps its stale fields out of the bar; the card carries the diagnostic.
    // 读取失败时其陈旧字段不进入状态栏；诊断由卡片承载。
    expect(render("deep_seek_balance", usage({
      availability: "temporarily_unavailable", balance_amount: "3", quota_windows: [countWindow("w", 1, 2)],
    })).innerHTML).toBe("")
  })

  it("puts the gauge before the balance when an account reports both", () => {
    const root = render("open_design_wallet", usage({
      balance_amount: "1.5", currency: "USD",
      quota_windows: [countWindow("short", 75, 100), countWindow("long", 80, 100)],
    }))
    const quota = root.querySelector('[data-slot="account-status-quota"]')
    const balance = root.querySelector('[data-slot="account-status-balance"]')
    expect(quota?.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("75")
    expect(balance?.textContent).toContain("1.50 USD")
    expect(quota?.compareDocumentPosition(balance as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING)
  })

  it("shows an unlimited plan without a gauge", () => {
    const root = render("kimi_coding_plan", usage({
      quota_windows: [{ ...countWindow("weekly_usage", 0, 0), remaining: undefined, total: undefined, unlimited: true }],
    }))
    expect(root.querySelector('[data-slot="account-status-quota"]')).not.toBeNull()
    expect(root.querySelector('[role="progressbar"]')).toBeNull()
  })
})
