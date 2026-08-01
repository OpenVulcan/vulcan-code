import * as React from "react"

import { useI18n, type TranslationKey } from "@/i18n"
import type {
  PublicAiAccountUsage,
  PublicAiAccountUsageWindow,
  PublicAiConnection,
  PublicAiCredentialProfile,
} from "@/types"

// CredentialUsageOverviewProps defines the normalized account-usage card body.
// CredentialUsageOverviewProps 定义规范化账号用量卡片主体。
export interface CredentialUsageOverviewProps {
  // connection identifies billing, provider, and optional management-key state.
  // connection 标识计费方式、供应商与可选管理密钥状态。
  connection: PublicAiConnection
  // credentialProfile declares whether usage lookup is supported.
  // credentialProfile 声明是否支持用量查询。
  credentialProfile?: PublicAiCredentialProfile
  // usage is the latest memory-only account observation.
  // usage 是最新的仅内存账号观测。
  usage?: PublicAiAccountUsage
  // showQuotaSection controls whether the surface renders quota content and its divider.
  // showQuotaSection 控制当前界面是否渲染额度内容及其分隔线。
  showQuotaSection?: boolean
}

// CredentialUsageOverview renders shared balance, plan, and quota presentation.
// CredentialUsageOverview 渲染共享的余额、套餐与额度展示。
export function CredentialUsageOverview(props: CredentialUsageOverviewProps) {
  // t resolves all account-usage copy in the active locale.
  // t 使用当前语言解析全部账号用量文案。
  const { t } = useI18n()
  // nonPlanAccount identifies providers whose balance account does not represent a subscription plan.
  // nonPlanAccount 标识余额账号不代表订阅套餐的供应商。
  const nonPlanAccount = props.connection.provider_id === "deepseek"
  // planName prefers the provider-reported plan unless the provider explicitly has no plan concept.
  // planName 优先使用供应商报告的套餐，除非该供应商明确没有套餐概念。
  const planName = nonPlanAccount
    ? t("composer.accountCard.nonPlan")
    : props.usage?.plan_name ?? credentialBillingKindLabel(props.connection.billing_kind, t)

  return (
    <>
      <div className="grid grid-cols-2 gap-2 border-t pt-1.5">
        <div className="min-w-0">
          <p className="text-[11px] text-muted-foreground">
            {t(nonPlanAccount ? "composer.accountCard.accountType" : "composer.accountCard.plan")}
          </p>
          <p className="mt-0.5 truncate text-sm font-medium">{planName}</p>
        </div>
        <div className="min-w-0">
          <p className="text-[11px] text-muted-foreground">{t("composer.accountCard.balance")}</p>
          <p className="mt-0.5 truncate text-sm font-medium">
            {accountBalanceLabel(props.usage)}
          </p>
        </div>
      </div>
      {props.showQuotaSection !== false ? (
        <div className="flex min-h-0 flex-1 flex-col border-t" data-slot="credential-usage">
          <div
            className="flex min-h-16 flex-1 items-center justify-center"
            data-slot="credential-usage-content"
          >
            {nonPlanAccount ? (
              <span className="text-center text-xs text-muted-foreground">
                {t("composer.accountCard.noQuotaWindows")}
              </span>
            ) : (
              <CredentialUsageSummary
                credentialProfile={props.credentialProfile}
                hasManagementKey={props.connection.has_management_key}
                providerId={props.connection.provider_id}
                usage={props.usage}
              />
            )}
          </div>
        </div>
      ) : null}
    </>
  )
}

// usageCapabilitySupportsQuotaWindows returns whether one account contract exposes plan windows.
// usageCapabilitySupportsQuotaWindows 返回一个账号契约是否公开套餐额度窗口。
//
// Parameters
// 参数
// `capability` is the exact backend-declared account usage implementation.
// `capability` 是后端声明的精确账号用量实现。
//
// Returns
// 返回值
// True only for usage implementations that produce user-facing quota windows.
// 仅对会产生用户可见额度窗口的用量实现返回 true。
export function usageCapabilitySupportsQuotaWindows(
  capability: PublicAiConnection["usage_capability"],
): boolean {
  return capability === "mini_max_token_plan"
    || capability === "tavily_credits"
    || capability === "kimi_coding_plan"
}

// CredentialUsageSummary renders provider-aware compact quota information.
// CredentialUsageSummary 渲染供应商感知的紧凑额度信息。
function CredentialUsageSummary(props: {
  // credentialProfile determines whether usage lookup exists.
  // credentialProfile 决定是否存在用量查询。
  credentialProfile?: PublicAiCredentialProfile
  // hasManagementKey indicates whether an optional balance credential is available.
  // hasManagementKey 表示可选余额查询凭据是否可用。
  hasManagementKey: boolean
  // providerId identifies the exact provider-specific renderer.
  // providerId 标识精确的供应商专用渲染器。
  providerId: string
  // usage is the latest exact account observation.
  // usage 是最新精确账号观测。
  usage?: PublicAiAccountUsage
}) {
  // t resolves provider-aware usage states in the active locale.
  // t 使用当前语言解析供应商感知的用量状态。
  const { t } = useI18n()
  if (props.credentialProfile?.account_usage_query === "unsupported") {
    return <span className="text-center text-xs text-muted-foreground">{t("composer.accountCard.usageUnsupported")}</span>
  }
  if (props.providerId === "openrouter" && !props.hasManagementKey) {
    return <span className="text-center text-xs text-muted-foreground">{t("composer.accountCard.managementKeyRequired")}</span>
  }
  if (!props.usage) {
    return <span className="text-center text-xs text-muted-foreground">{t("composer.accountCard.usageNotQueried")}</span>
  }
  if (props.usage.availability === "temporarily_unavailable") {
    return (
      <span className="text-center text-xs text-muted-foreground">
        {props.usage.refresh_message ?? t("composer.accountCard.usageUnavailable")}
      </span>
    )
  }
  if (!Array.isArray(props.usage.quota_windows)) {
    return <span className="text-center text-xs text-destructive">{t("composer.accountCard.usageInvalid")}</span>
  }
  // rows joins MiniMax rolling and weekly windows into resource buckets.
  // rows 将 MiniMax 滚动周期与周周期组合为资源桶。
  const rows = props.providerId === "minimax"
    ? miniMaxUsageRows(props.usage.quota_windows)
    : []
  if (rows.length > 0) {
    return <MiniMaxUsageSummary rows={rows} />
  }
  // tavilyPlanWindow selects only the account-plan allowance used by the compact Tavily card.
  // tavilyPlanWindow 仅选择 Tavily 紧凑卡片使用的账号套餐额度。
  const tavilyPlanWindow = props.providerId === "tavily"
    ? props.usage.quota_windows.find((window) => window.id === "tavily-account-plan")
    : undefined
  if (props.providerId === "tavily") {
    return <TavilyUsageSummary window={tavilyPlanWindow} />
  }
  if (props.usage.quota_windows.length === 0) {
    return <span className="text-center text-xs text-muted-foreground">{t("composer.accountCard.noQuotaWindows")}</span>
  }
  return (
    <div className="w-full space-y-1.5">
      {props.usage.quota_windows.slice(0, 3).map((window) => (
        <UsageWindowMetric key={window.id} label={window.display_name} window={window} />
      ))}
    </div>
  )
}

// TavilyUsageSummary renders only provider-reported account-plan consumption.
// TavilyUsageSummary 仅渲染供应商报告的账号套餐消费。
function TavilyUsageSummary(props: {
  // window is the exact normalized Tavily account-plan allowance.
  // window 是精确规范化的 Tavily 账号套餐额度。
  window?: PublicAiAccountUsageWindow
}) {
  // t resolves the compact Tavily labels in the active locale.
  // t 使用当前语言解析 Tavily 紧凑标签。
  const { t } = useI18n()
  // total determines whether the provider supplied a finite plan allowance.
  // total 决定供应商是否提供了有限套餐额度。
  const total = Number(props.window?.total)
  // hasFiniteLimit distinguishes a real plan cap from an absent independent limit.
  // hasFiniteLimit 区分真实套餐上限与缺失的独立上限。
  const hasFiniteLimit = Number.isFinite(total) && total > 0
  // value preserves Tavily's used-credit semantics instead of substituting remaining credits.
  // value 保留 Tavily 已用 Credit 语义，而不以剩余额度替代。
  const value = props.window?.used === undefined
    ? "—"
    : hasFiniteLimit
      ? `${props.window.used} / ${props.window.total}`
      : props.window.used
  // percentage is the normalized remaining-plan percentage used by the progress bar.
  // percentage 是进度条使用的规范化套餐剩余百分比。
  const percentage = usageWindowPercentage(props.window)
  return (
    <div className="w-full space-y-1 text-[11px]">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium">{t("composer.accountCard.tavilyPlanQuota")}</span>
        <span className="tabular-nums">
          {t("composer.accountCard.used")} {value}
        </span>
      </div>
      {hasFiniteLimit ? (
        <div
          aria-label={`${t("composer.accountCard.tavilyPlanQuota")}: ${Math.round(percentage)}%`}
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={Math.round(percentage)}
          className="h-1.5 overflow-hidden rounded-full bg-muted"
          role="progressbar"
        >
          <div
            className={`h-full rounded-full ${percentage <= 10 ? "bg-destructive" : percentage <= 30 ? "bg-amber-500" : "bg-emerald-500"}`}
            style={{ width: `${percentage}%` }}
          />
        </div>
      ) : null}
    </div>
  )
}

// MiniMaxUsageRow joins current and weekly windows for one resource bucket.
// MiniMaxUsageRow 将一个资源桶的当前周期与周周期连接起来。
interface MiniMaxUsageRow {
  // current is the provider's short rolling window.
  // current 是供应商短滚动周期。
  current?: PublicAiAccountUsageWindow
  // name is the exact provider resource bucket name.
  // name 是精确供应商资源桶名称。
  name: string
  // weekly is the provider's weekly window.
  // weekly 是供应商周周期。
  weekly?: PublicAiAccountUsageWindow
}

// miniMaxUsageRows recognizes the exact normalized MiniMax window identifiers.
// miniMaxUsageRows 识别精确规范化的 MiniMax 窗口标识。
function miniMaxUsageRows(windows: PublicAiAccountUsageWindow[]): MiniMaxUsageRow[] {
  // rows accumulates exact resource buckets without inventing unavailable windows.
  // rows 累积精确资源桶且不虚构不可用窗口。
  const rows = new Map<string, MiniMaxUsageRow>()
  for (const window of windows) {
    // match recognizes the normalized current/weekly identifier suffix.
    // match 识别规范化的 current/weekly 标识后缀。
    const match = /^(.+):(current|weekly)$/.exec(window.id)
    if (!match || !window.bucket_name) {
      continue
    }
    // row reuses the bucket accumulated for this exact resource name.
    // row 复用为该精确资源名称累积的资源桶。
    const row = rows.get(window.bucket_name) ?? { name: window.bucket_name }
    if (match[2] === "current") {
      row.current = window
    } else {
      row.weekly = window
    }
    rows.set(row.name, row)
  }
  return [...rows.values()]
    .filter((row) => row.current?.available === true || row.weekly?.available === true)
    .sort((left, right) => (
      miniMaxBucketOrder(left.name) - miniMaxBucketOrder(right.name)
      || left.name.localeCompare(right.name)
    ))
}

// MiniMaxUsageSummary renders paired MiniMax Token Plan quota windows.
// MiniMaxUsageSummary 渲染成对的 MiniMax Token Plan 额度窗口。
function MiniMaxUsageSummary(props: { rows: MiniMaxUsageRow[] }) {
  // t resolves the paired-window labels in the active locale.
  // t 使用当前语言解析成对窗口标签。
  const { t } = useI18n()
  return (
    <div className="w-full space-y-1.5">
      {props.rows.map((row) => (
        <div
          className="grid grid-cols-[1.75rem_minmax(0,1fr)_minmax(0,1fr)_auto] items-center gap-1 text-[11px]"
          key={row.name}
        >
          <span className="font-medium">{miniMaxBucketLabel(row.name, t)}</span>
          <UsageWindowMetric label={t("composer.accountCard.remaining")} window={row.current} />
          <UsageWindowMetric label={t("composer.accountCard.weeklyRemaining")} window={row.weekly} />
          <span className="whitespace-nowrap tabular-nums text-muted-foreground">
            {t("composer.accountCard.reset")} {usageResetDuration(row.current?.resets_at)}
          </span>
        </div>
      ))}
    </div>
  )
}

// UsageWindowMetric renders one exact remaining value and compact progress indicator.
// UsageWindowMetric 渲染一个精确剩余值和紧凑进度指示器。
function UsageWindowMetric(props: {
  // label names the quota window.
  // label 命名额度周期。
  label: string
  // window is the exact normalized provider observation.
  // window 是精确规范化供应商观测。
  window?: PublicAiAccountUsageWindow
}) {
  // t resolves quota value labels in the active locale.
  // t 使用当前语言解析额度值标签。
  const { t } = useI18n()
  // percentage is the normalized progress value for the visual bar.
  // percentage 是用于视觉进度条的规范化进度值。
  const percentage = usageWindowPercentage(props.window)
  // value preserves the provider's exact quota semantics.
  // value 保留供应商的精确额度语义。
  const value = usageWindowValue(props.window, t)
  return (
    <div className="min-w-0 space-y-0.5">
      <div className="flex items-center justify-between gap-1">
        <span className="truncate text-muted-foreground">{props.label}</span>
        <span className="shrink-0 tabular-nums">{value}</span>
      </div>
      <div
        aria-label={`${props.label}: ${value}`}
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={percentage}
        className="h-1.5 overflow-hidden rounded-full bg-muted"
        role="progressbar"
      >
        <div
          className={`h-full rounded-full ${percentage <= 10 ? "bg-destructive" : percentage <= 30 ? "bg-amber-500" : "bg-emerald-500"}`}
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  )
}

// usageWindowPercentage returns the normalized visual percentage without inventing quota.
// usageWindowPercentage 返回规范化视觉百分比且不虚构额度。
function usageWindowPercentage(window?: PublicAiAccountUsageWindow): number {
  if (!window || !window.available) {
    return 0
  }
  if (window.unlimited) {
    return 100
  }
  // explicit is the provider-reported remaining percentage.
  // explicit 是供应商报告的剩余百分比。
  const explicit = Number(window.remaining_percent)
  if (Number.isFinite(explicit)) {
    return Math.max(0, Math.min(100, explicit))
  }
  // remaining and total preserve exact count-based quota semantics.
  // remaining 与 total 保留基于次数的精确额度语义。
  const remaining = Number(window.remaining)
  const total = Number(window.total)
  if (Number.isFinite(remaining) && Number.isFinite(total) && total > 0) {
    return Math.max(0, Math.min(100, (remaining / total) * 100))
  }
  return 0
}

// usageWindowValue renders exact counts, percentage, unlimited, or unavailable state.
// usageWindowValue 渲染精确次数、百分比、无限或不可用状态。
function usageWindowValue(
  window: PublicAiAccountUsageWindow | undefined,
  t: (key: TranslationKey) => string,
): string {
  if (!window) {
    return "—"
  }
  if (!window.available) {
    return t("composer.accountCard.notIncluded")
  }
  if (window.unlimited) {
    return t("composer.accountCard.unlimited")
  }
  // total determines whether exact remaining/total counts are available.
  // total 决定是否存在精确的剩余数/总数。
  const total = Number(window.total)
  if (Number.isFinite(total) && total > 0) {
    return `${window.remaining ?? "0"} / ${window.total}`
  }
  return window.remaining_percent !== undefined
    ? `${window.remaining_percent}%`
    : "—"
}

// usageResetDuration renders the provider reset time as a compact countdown.
// usageResetDuration 将供应商重置时间渲染为紧凑倒计时。
function usageResetDuration(value?: string): string {
  if (!value) {
    return "—"
  }
  // numeric distinguishes epoch timestamps from ISO date strings.
  // numeric 区分纪元时间戳与 ISO 日期字符串。
  const numeric = Number(value)
  // resetMilliseconds normalizes seconds, milliseconds, and ISO dates.
  // resetMilliseconds 规范化秒、毫秒与 ISO 日期。
  const resetMilliseconds = Number.isFinite(numeric)
    ? numeric * (numeric < 1_000_000_000_000 ? 1000 : 1)
    : new Date(value).getTime()
  if (!Number.isFinite(resetMilliseconds)) {
    return "—"
  }
  // remainingMilliseconds is clamped because expired windows reset immediately.
  // remainingMilliseconds 进行下限约束，因为过期窗口应立即重置。
  const remainingMilliseconds = Math.max(0, resetMilliseconds - Date.now())
  // hours and minutes form the compact countdown.
  // hours 与 minutes 组成紧凑倒计时。
  const hours = Math.floor(remainingMilliseconds / 3_600_000)
  const minutes = Math.floor((remainingMilliseconds % 3_600_000) / 60_000)
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`
}

// miniMaxBucketOrder keeps the provider's primary resource buckets stable.
// miniMaxBucketOrder 保持供应商主要资源桶顺序稳定。
function miniMaxBucketOrder(name: string): number {
  return name === "general" ? 0 : name === "video" ? 1 : 2
}

// miniMaxBucketLabel localizes known MiniMax resource buckets.
// miniMaxBucketLabel 本地化已知 MiniMax 资源桶。
function miniMaxBucketLabel(
  name: string,
  t: (key: TranslationKey) => string,
): string {
  return name === "general"
    ? t("composer.accountCard.general")
    : name === "video"
      ? t("composer.accountCard.video")
      : name
}

// credentialBillingKindLabel renders the persisted credential billing contract.
// credentialBillingKindLabel 渲染持久化凭据计费契约。
function credentialBillingKindLabel(
  kind: PublicAiConnection["billing_kind"],
  t: (key: TranslationKey) => string,
): string {
  if (kind === "token_plan") {
    return t("composer.accountCard.billingTokenPlan")
  }
  if (kind === "pay_as_you_go") {
    return t("composer.accountCard.billingPayAsYouGo")
  }
  return t("composer.accountCard.billingProviderManaged")
}

// accountBalanceLabel renders exact balance and currency without numeric coercion.
// accountBalanceLabel 渲染精确余额和币种且不进行数值强制转换。
function accountBalanceLabel(usage?: PublicAiAccountUsage): string {
  if (!usage?.balance_amount) {
    return "--"
  }
  // formattedAmount rounds the provider decimal string without converting it to a floating-point Number.
  // formattedAmount 在不转换为浮点 Number 的情况下舍入供应商金额字符串。
  const formattedAmount = formatCurrencyAmount(usage.balance_amount)
  return usage.currency
    ? `${formattedAmount} ${usage.currency}`
    : formattedAmount
}

// formatCurrencyAmount formats a plain decimal currency string with exactly two fractional digits.
// formatCurrencyAmount 将普通十进制金额字符串格式化为固定两位小数。
//
// Parameters
// 参数
// `value` is the provider-owned decimal string and is never converted to JavaScript Number.
// `value` 是供应商返回的十进制字符串，绝不会转换为 JavaScript Number。
//
// Returns
// 返回值
// A half-up rounded two-decimal string, or the original value for an unsupported representation.
// 返回四舍五入后的两位小数字符串；不支持的表示形式保持原值。
export function formatCurrencyAmount(value: string): string {
  const match = /^([+-]?)(\d+)(?:\.(\d*))?$/.exec(value.trim())
  if (!match) {
    return value
  }
  const sign = match[1]
  const integerDigits = match[2]
  const fractionDigits = match[3] ?? ""
  const retainedFraction = fractionDigits.padEnd(2, "0").slice(0, 2)
  const roundingDigit = fractionDigits[2] ?? "0"
  let scaledAmount = BigInt(`${integerDigits}${retainedFraction}`)
  if (roundingDigit >= "5") {
    scaledAmount += 1n
  }
  const whole = scaledAmount / 100n
  const fraction = String(scaledAmount % 100n).padStart(2, "0")
  const normalizedSign = sign === "-" && scaledAmount !== 0n ? "-" : sign === "+" ? "+" : ""
  return `${normalizedSign}${whole}.${fraction}`
}
