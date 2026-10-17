// @vitest-environment jsdom

import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vitest"

import type { PublicAiAccountUsage, PublicAiConnection, PublicAiCredentialProfile } from "@/types"
import { CredentialUsageOverview } from "./credential-usage-overview"

// Catalog keys expose semantic rendering choices without asserting translated prose.
// 目录键公开语义渲染选择，不断言翻译文案。
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }))

// Synthetic connection contains only the public metadata consumed by the shared account card.
// 合成连接只包含共享账户卡片使用的公开元数据。
const connection: PublicAiConnection = {
  id: "synthetic-connection", provider_id: "open-design", service_entry_id: "design-plan",
  credential_profile_id: "account-auth", billing_kind: "token_plan", usage_capability: "open_design_wallet",
  usage_requires_management_key: false, has_management_key: false, credential_mode: "auth",
  credential_id: "synthetic-credential", display_name: "Synthetic account", enabled: true,
  prompt_cache_by_protocol: {}, pending_validation: false,
}

// The declared usage capability owns quota rendering, independently of provider names.
// 声明的用量能力持有额度渲染行为，不依赖供应商名称。
const profile: PublicAiCredentialProfile = {
  id: "account-auth", display_name: "Synthetic profile", credential_mode: "auth",
  billing_kind: "token_plan", account_usage_query: "open_design_wallet",
}

// Usage keeps wallet money separate from two simultaneous plan allowances.
// 用量将钱包金额与两个并行套餐额度分开。
const usage: PublicAiAccountUsage = {
  credential_id: "synthetic-credential", availability: "supported", plan_name: "Go",
  currency: "USD", balance_amount: "1.50",
  quota_windows: [
    { id: "short-policy", display_name: "Design Plan", duration_seconds: 18000,
      remaining: "75", total: "100", unit: "credits", available: true, unlimited: false,
      resets_at: "2030-01-01T05:00:00Z" },
    { id: "long-policy", display_name: "Design Plan", duration_seconds: 604800,
      remaining: "80", total: "100", unit: "credits", available: true, unlimited: false,
      resets_at: "2030-01-08T00:00:00Z" },
  ],
}

// Renders one supplied observation through the actual shared card; returns its detached DOM for assertions.
// 通过真实共享卡片渲染传入观测；返回独立 DOM 供断言使用。
function renderUsage(observation: PublicAiAccountUsage): HTMLDivElement {
  // Root contains only synthetic server-rendered markup and never mounts a polling timer.
  // Root 只包含合成服务端渲染标记，不挂载轮询定时器。
  const root = document.createElement("div")
  root.innerHTML = renderToStaticMarkup(
    <CredentialUsageOverview connection={connection} credentialProfile={profile} usage={observation} showQuotaSection />,
  )
  return root
}

describe("OpenDesign account card", () => {
  // Verify both limits remain separate from the premium-wallet fact and retain their reset displays.
  // 验证两个限制与高级模型钱包事实彼此独立，并保留各自的重置显示。
  it("renders both quota bars beside a separately labelled premium balance", () => {
    // DOM assertions follow roles and stable slots rather than localized text or snapshot positions.
    // DOM 断言跟随角色及稳定槽位，而不是本地化文案或快照位置。
    const root = renderUsage(usage)
    // Progress roles expose each independently rendered allowance.
    // 进度角色公开每个独立渲染的额度。
    const bars = Array.from(root.querySelectorAll('[role="progressbar"]'))
    expect(bars.map((bar) => bar.getAttribute("aria-valuenow")).sort()).toEqual(["75", "80"])
    expect(root.querySelectorAll('[data-slot="quota-window-reset"]')).toHaveLength(2)
    expect(root.textContent).toContain("composer.accountCard.premiumBalance")
    expect(root.textContent).toContain(usage.plan_name)
    expect(root.textContent).toContain("1.50 USD")
    expect(root.textContent).not.toContain("75 / 100")
  })

  // A failed complete observation must not masquerade as usable plan quota.
  // 完整观测失败不得伪装成可用套餐额度。
  it("removes quota bars when account usage is temporarily unavailable", () => {
    // Failed reads retain identity but use the explicit unavailable presentation path.
    // 失败读取保留身份，但使用明确的不可用展示路径。
    const root = renderUsage({ ...usage, availability: "temporarily_unavailable" })
    expect(root.querySelector('[role="progressbar"]')).toBeNull()
    expect(root.textContent).toContain("composer.accountCard.usageUnavailable")
  })
})

describe("OpenCode account card", () => {
  // An absent paid period is an account state, distinct from a provider lacking usage support.
  // 缺失付费周期属于账号状态，与供应商不支持用量查询不同。
  it("shows an inactive allowance when the workspace has no active subscription windows", () => {
    // root renders an authoritative successful observation with no active windows.
    // root 渲染权威成功观测，其不含有效窗口。
    const root = document.createElement("div")
    root.innerHTML = renderToStaticMarkup(
      <CredentialUsageOverview
        connection={{ ...connection, usage_capability: "opencode_go" }}
        credentialProfile={{ ...profile, account_usage_query: "opencode_go" }}
        usage={{ ...usage, quota_windows: [] }}
        showQuotaSection
      />,
    )
    expect(root.querySelector('[role="progressbar"]')).toBeNull()
    expect(root.textContent).toContain("composer.accountCard.opencodeNoActiveQuota")
    expect(root.textContent).not.toContain("composer.accountCard.noQuotaWindows")
  })

  // Each localized meter retains its own monetary allowance and source reset time beside the account balance.
  // 每个本地化窗口在账号余额旁保留自身金额额度与来源重置时间。
  it("renders three localized quota windows and an independent credit balance", () => {
    // observation uses only the three identities emitted by the Console adapter.
    // observation 只使用 Console 适配器产出的三个身份。
    const observation: PublicAiAccountUsage = {
      ...usage, plan_name: "OpenCode Go Plus", balance_amount: "1.50",
      quota_windows: [
        { id: "five-hour", display_name: "Provider five-hour label", total: "20", remaining: "15",
          unit: "USD", unlimited: false, available: true },
        { id: "week", display_name: "Provider weekly label", total: "50", remaining: "40",
          unit: "USD", unlimited: false, available: true, resets_at: "2030-01-07T00:00:00Z" },
        { id: "month", display_name: "Provider monthly label", total: "100", remaining: "90",
          unit: "USD", unlimited: false, available: true, resets_at: "2030-02-01T00:00:00Z" },
      ],
    }
    // root follows the same shared card used by account settings and the conversation selector.
    // root 使用账号设置与会话选择器共用的卡片。
    const root = document.createElement("div")
    root.innerHTML = renderToStaticMarkup(
      <CredentialUsageOverview
        connection={{ ...connection, provider_id: "opencode-go", usage_capability: "opencode_go" }}
        credentialProfile={{ ...profile, account_usage_query: "opencode_go" }}
        usage={observation}
        showQuotaSection
      />,
    )
    expect(root.querySelectorAll('[role="progressbar"]')).toHaveLength(observation.quota_windows.length)
    expect(root.querySelectorAll('[data-slot="quota-window-reset"]')).toHaveLength(
      observation.quota_windows.filter((window) => window.resets_at).length,
    )
    for (const key of ["opencodeFiveHourQuota", "opencodeWeeklyQuota", "opencodeMonthlyQuota"]) {
      expect(root.textContent).toContain(`composer.accountCard.${key}`)
    }
    expect(root.textContent).toContain("composer.accountCard.balance")
    expect(root.textContent).toContain("1.50 USD")
    expect(root.textContent).not.toContain("Provider weekly label")
  })
})
