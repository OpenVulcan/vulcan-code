import "@/ui-kit/styles/features/account-status.css"

import { UsageMeter, type UsageMeterTone } from "@/components/product/usage"
import { useI18n, type TranslationKey } from "@/i18n"
import type { PublicAiAccountUsageWindow, PublicAiCredentialProfile } from "@/types"
import {
  accountBalanceLabel,
  codexUsageWindowLabel,
  commandCodeUsageWindowLabel,
  kimiUsageWindowLabel,
  openDesignUsageWindowLabel,
  opencodeUsageWindowLabel,
  usageMeterTone,
  usageWindowPercentage,
  type CredentialUsageOverviewProps,
} from "./credential-usage-overview"

// STATUS_USAGE_METER_CELLS keeps the status-bar gauge legible at its narrow fixed width.
// STATUS_USAGE_METER_CELLS 使状态栏仪表在较窄的固定宽度下仍然清晰可读。
const STATUS_USAGE_METER_CELLS = 10

// SUPPLEMENTARY_CREDIT_WINDOW_IDS are CommandCode pools that top up the monthly credits instead of limiting them.
// SUPPLEMENTARY_CREDIT_WINDOW_IDS 是补充每月额度而非限制每月额度的 CommandCode 额度池。
const SUPPLEMENTARY_CREDIT_WINDOW_IDS: ReadonlySet<string> = new Set(["purchased-credits", "free-credits"])

// CredentialUsageStatusQuota is the one quota window the status bar condenses into a gauge.
// CredentialUsageStatusQuota 是状态栏压缩为仪表的单个额度窗口。
export interface CredentialUsageStatusQuota {
  // windowLabel is the localized name of the selected window, used for the accessible gauge name.
  // windowLabel 是所选窗口的本地化名称，用作仪表的无障碍名称。
  windowLabel: string
  // value is the compact visible text: the remaining share or the unlimited label.
  // value 是紧凑的可见文字：剩余比例或无限标签。
  value: string
  // percentage is the remaining share for the gauge; null for unlimited windows, which show no gauge.
  // percentage 是仪表使用的剩余比例；无限窗口为 null 且不显示仪表。
  percentage: number | null
  // tone grades the remaining share by the shared usage thresholds.
  // tone 按共享用量阈值对剩余比例分档。
  tone: UsageMeterTone
}

// CredentialUsageStatusBalance is the localized balance fact shown in the status bar.
// CredentialUsageStatusBalance 是状态栏显示的本地化余额事实。
export interface CredentialUsageStatusBalance {
  // label is the localized balance metric name.
  // label 是本地化的余额指标名称。
  label: string
  // value is the exact formatted amount with its currency.
  // value 是带币种的精确格式化金额。
  value: string
}

// CredentialUsageStatusFacts lists what the account actually reports; both null means only the account is shown.
// CredentialUsageStatusFacts 列出账号实际报告的内容；两者都为 null 时只显示账号。
export interface CredentialUsageStatusFacts {
  quota: CredentialUsageStatusQuota | null
  balance: CredentialUsageStatusBalance | null
}

// credentialUsageStatusFacts selects the compact facts of one account for the status bar.
// credentialUsageStatusFacts 为状态栏选出一个账号的紧凑事实。
// Parameters: props supplies the account facts and t resolves catalog keys.
// 参数：props 提供账号事实，t 解析目录键。
// Returns: the most constrained determinable quota window and the reported balance, each null when absent;
// failed or unqueried observations report neither.
// 返回值：可确定的最紧张额度窗口与已报告余额，缺失时各为 null；失败或未查询的观测两者都不报告。
export function credentialUsageStatusFacts(
  props: Pick<CredentialUsageOverviewProps, "credentialProfile" | "usage">,
  t: (key: TranslationKey) => string,
): CredentialUsageStatusFacts {
  const usage = props.usage
  if (!usage || usage.availability !== "supported") {
    return { quota: null, balance: null }
  }
  const query = props.credentialProfile?.account_usage_query
  const balance = usage.balance_amount?.trim()
    ? {
      label: t(query === "open_design_wallet" ? "composer.accountCard.premiumBalance" : "composer.accountCard.balance"),
      value: accountBalanceLabel(usage),
    }
    : null
  const windows = Array.isArray(usage.quota_windows) ? statusCandidateWindows(usage.quota_windows, query) : []
  // binding is the window with the smallest remaining share, which limits the account first.
  // binding 是剩余比例最小的窗口，它最先限制账号。
  let binding: { window: PublicAiAccountUsageWindow; percentage: number } | null = null
  for (const window of windows) {
    const percentage = usageWindowPercentage(window)
    if (percentage !== null && (!binding || percentage < binding.percentage)) {
      binding = { window, percentage }
    }
  }
  if (!binding) {
    return { quota: null, balance }
  }
  const windowLabel = statusWindowLabel(binding.window, query, t)
  if (binding.window.unlimited) {
    return { quota: { windowLabel, value: t("composer.accountCard.unlimited"), percentage: null, tone: "ok" }, balance }
  }
  // Floor never overstates what is left, matching the lit cells of the gauge.
  // 向下取整从不高估剩余量，与仪表点亮的格数一致。
  const shown = Math.floor(binding.percentage)
  return {
    quota: {
      windowLabel,
      value: `${t("composer.accountCard.remaining")} ${shown}%`,
      percentage: binding.percentage,
      tone: usageMeterTone(binding.percentage),
    },
    balance,
  }
}

// statusCandidateWindows keeps the windows that limit the account's main use, so side buckets and top-up pools
// never stand in for the plan.
// statusCandidateWindows 保留限制账号主要用途的窗口，使附属额度桶与补充额度池不会代替套餐显示。
// Parameters: windows are the normalized provider windows and query the declared usage contract.
// 参数：windows 为规范化供应商窗口，query 为声明的用量契约。
// Returns: the candidate windows, never empty while windows is not.
// 返回值：候选窗口；windows 非空时绝不为空。
function statusCandidateWindows(
  windows: PublicAiAccountUsageWindow[],
  query: PublicAiCredentialProfile["account_usage_query"] | undefined,
): PublicAiAccountUsageWindow[] {
  const available = windows.filter((window) => window.available)
  // preferred narrows to the main allowance whenever the contract names one that is present.
  // preferred 在契约所指的主要额度存在时收窄到该额度。
  const preferred = query === "mini_max_token_plan"
    ? available.filter((window) => window.bucket_name === "general")
    : query === "tavily_credits"
      ? available.filter((window) => window.id === "tavily-account-plan")
      : query === "command_code_usage"
        ? available.filter((window) => !SUPPLEMENTARY_CREDIT_WINDOW_IDS.has(window.id))
        : available.filter((window) => !window.bucket_name)
  return preferred.length > 0 ? preferred : available
}

// statusWindowLabel names one window with the same localized labels as the full usage card.
// statusWindowLabel 使用与完整用量卡片相同的本地化标签命名一个窗口。
function statusWindowLabel(
  window: PublicAiAccountUsageWindow,
  query: PublicAiCredentialProfile["account_usage_query"] | undefined,
  t: (key: TranslationKey) => string,
): string {
  switch (query) {
    case "kimi_coding_plan": return kimiUsageWindowLabel(window, t)
    case "command_code_usage": return commandCodeUsageWindowLabel(window, t)
    case "opencode_go": return opencodeUsageWindowLabel(window, t)
    case "open_ai_codex_usage": return codexUsageWindowLabel(window, t)
    case "open_design_wallet": return openDesignUsageWindowLabel(window, t)
    case "tavily_credits": return t("composer.accountCard.tavilyPlanQuota")
    default: return window.display_name
  }
}

// CredentialUsageStatusLine renders the status-bar fragment after the account name: a quota gauge, a balance, both
// (gauge first, balance truncating), or nothing when the account reports neither.
// CredentialUsageStatusLine 渲染状态栏中账号名之后的片段：额度仪表、余额、两者兼有（仪表在前、余额可截断），
// 账号两者都未报告时不渲染任何内容。
// Parameters: props supplies the account facts. Returns the fragment, or null.
// 参数：props 提供账号事实。返回该片段或 null。
export function CredentialUsageStatusLine(props: CredentialUsageOverviewProps) {
  const { t } = useI18n()
  const facts = credentialUsageStatusFacts(props, t)
  if (!facts.quota && !facts.balance) {
    return null
  }
  return (
    <span className="vk-status-plan" data-slot="account-status-usage">
      {facts.quota ? (
        <span className="vk-status-quota" data-slot="account-status-quota" data-tone={facts.quota.tone}>
          {facts.quota.percentage !== null ? (
            <UsageMeter
              cells={STATUS_USAGE_METER_CELLS}
              label={`${facts.quota.windowLabel}: ${facts.quota.value}`}
              percentage={facts.quota.percentage}
              tone={facts.quota.tone}
            />
          ) : null}
          <span>{facts.quota.value}</span>
        </span>
      ) : null}
      {facts.balance ? (
        <span className="vk-status-balance" data-slot="account-status-balance">
          {`${facts.balance.label} ${facts.balance.value}`}
        </span>
      ) : null}
    </span>
  )
}
