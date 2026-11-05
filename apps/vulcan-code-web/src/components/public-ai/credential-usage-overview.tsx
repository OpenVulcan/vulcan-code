import * as React from "react"

import { UsageMeter, type UsageMeterTone } from "@/components/product/usage"
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

// CredentialUsageOverviewViewModel contains only localized, already-formatted header facts.
// CredentialUsageOverviewViewModel 只包含已本地化且已经格式化的头部事实。
export interface CredentialUsageOverviewViewModel {
  // balanceLabel is the localized balance metric name.
  // balanceLabel 是已本地化的余额指标名称。
  balanceLabel: string
  // balanceValue is the exact formatted provider balance.
  // balanceValue 是精确格式化后的供应商余额。
  balanceValue: string
  // planLabel is the localized plan or account-type metric name.
  // planLabel 是已本地化的套餐或账号类型指标名称。
  planLabel: string
  // planValue is the exact visible plan or account-type value.
  // planValue 是精确可见的套餐或账号类型值。
  planValue: string
}

// CredentialUsageOverviewViewProps combines determined header facts with adapted quota content.
// CredentialUsageOverviewViewProps 将确定的头部事实与已适配额度内容组合。
export interface CredentialUsageOverviewViewProps {
  // model contains already-formatted header facts.
  // model 包含已经格式化的头部事实。
  model: CredentialUsageOverviewViewModel
  // quotaContent contains provider-specific quota presentation from the business adapter.
  // quotaContent 包含来自业务适配层的供应商专用额度展示。
  quotaContent?: React.ReactNode
  // showQuotaSection controls whether the quota region and separator are visible.
  // showQuotaSection 控制额度区域及分隔线是否可见。
  showQuotaSection: boolean
}

// credentialUsageIsNonPlanAccount resolves account presentation from manifest-projected semantics.
// credentialUsageIsNonPlanAccount 从 Manifest 投影语义解析账号展示方式。
export function credentialUsageIsNonPlanAccount(
  credentialProfile: PublicAiCredentialProfile | undefined,
): boolean {
  return credentialProfile?.account_usage_query === "deep_seek_balance"
    || credentialProfile?.account_usage_query === "node_ai_balance"
}

// CredentialUsageOverview renders shared balance, plan, and quota presentation.
// CredentialUsageOverview 渲染共享的余额、套餐与额度展示。
export function CredentialUsageOverview(props: CredentialUsageOverviewProps) {
  // overview holds the localized header facts and adapted quota content for the pure view.
  // overview 持有供纯视图使用的本地化头部事实与已适配额度内容。
  const overview = useCredentialUsageOverview(props)
  return (
    <CredentialUsageOverviewView
      model={overview.model}
      quotaContent={overview.quotaContent}
      showQuotaSection={props.showQuotaSection !== false}
    />
  )
}

// useCredentialUsageOverview adapts one account into header facts and quota content so callers can place them separately.
// useCredentialUsageOverview 将一个账号适配为头部事实与额度内容，使调用方可以分开放置二者。
// Parameters: props supplies the account facts; returns the localized view model and the quota node.
// 参数：props 提供账号事实；返回本地化视图模型与额度节点。
export function useCredentialUsageOverview(props: CredentialUsageOverviewProps): {
  model: CredentialUsageOverviewViewModel
  quotaContent: React.ReactNode
} {
  // t resolves all account-usage copy in the active locale.
  // t 使用当前语言解析全部账号用量文案。
  const { t } = useI18n()
  // nonPlanAccount derives presentation semantics from the manifest-projected credential profile.
  // nonPlanAccount 从 Manifest 投影的凭据配置档推导展示语义。
  const nonPlanAccount = credentialUsageIsNonPlanAccount(props.credentialProfile)
  // planName prefers the provider-reported plan unless the provider explicitly has no plan concept.
  // planName 优先使用供应商报告的套餐，除非该供应商明确没有套餐概念。
  const planName = nonPlanAccount
    ? t("composer.accountCard.nonPlan")
    : props.usage?.plan_name ?? credentialBillingKindLabel(props.connection.billing_kind, t)

  // model converts account facts into the exact localized values consumed by the pure view.
  // model 将账号事实转换为纯视图消费的精确本地化值。
  const model: CredentialUsageOverviewViewModel = {
    balanceLabel: t(props.credentialProfile?.account_usage_query === "open_design_wallet"
      ? "composer.accountCard.premiumBalance"
      : "composer.accountCard.balance"),
    balanceValue: accountBalanceLabel(props.usage),
    planLabel: t(nonPlanAccount ? "composer.accountCard.accountType" : "composer.accountCard.plan"),
    planValue: planName,
  }
  // quotaContent adapts the declared provider capability before entering the pure display layer.
  // quotaContent 在进入纯展示层前适配已声明的供应商能力。
  // Only a successful balance observation can suppress quota details; failed reads retain their diagnostic.
  // 只有成功余额观测可以省略额度详情；查询失败仍保留诊断信息。
  const quotaContent = nonPlanAccount && props.usage?.availability === "supported" ? (
    <span className="vk-usage-hint">
      {t("composer.accountCard.noQuotaWindows")}
    </span>
  ) : (
    <CredentialUsageSummary
      credentialProfile={props.credentialProfile}
      hasManagementKey={props.connection.has_management_key}
      usageRequiresManagementKey={props.connection.usage_requires_management_key}
      usage={props.usage}
    />
  )

  return { model, quotaContent }
}

// CredentialUsageSummaryLine condenses plan and balance into one line of text.
// CredentialUsageSummaryLine 将套餐与余额压缩为一行文字。
// Parameters: props supplies the account facts; returns "plan · balance" with the same localized facts as the card.
// 参数：props 提供账号事实；返回与卡片相同本地化事实的“套餐 · 余额”。
export function CredentialUsageSummaryLine(props: CredentialUsageOverviewProps) {
  const { model } = useCredentialUsageOverview(props)
  return <>{`${model.planValue} · ${model.balanceLabel} ${model.balanceValue}`}</>
}

// CredentialUsageOverviewView renders only determined account view data and adapted quota content.
// CredentialUsageOverviewView 只渲染确定的账号视图数据与已适配额度内容。
// Parameters: props supplies adapted account usage and pricing facts; returns known values and explicit unknown states without issuing queries.
// 参数：props 提供已适配账号用量与定价事实；返回已知值和明确未知状态，不发起查询。
export function CredentialUsageOverviewView(props: CredentialUsageOverviewViewProps) {
  return (
    <CredentialUsageQuotaPanel header={<CredentialUsageInlineFacts model={props.model} />}>
      {props.showQuotaSection ? props.quotaContent : null}
    </CredentialUsageQuotaPanel>
  )
}

// CredentialUsageInlineFacts renders plan and balance as one wrapping row of label-value pairs.
// CredentialUsageInlineFacts 将套餐与余额渲染为一行可换行的标签-值对。
// Parameters: props.model supplies the formatted facts; returns a compact description list.
// 参数：props.model 提供已格式化事实；返回紧凑的描述列表。
export function CredentialUsageInlineFacts(props: { model: CredentialUsageOverviewViewModel }) {
  return (
    <dl className="vk-usage-facts" data-slot="credential-usage-facts">
      <div>
        <dt>{props.model.planLabel}</dt>
        <dd>{props.model.planValue}</dd>
      </div>
      <div>
        <dt>{props.model.balanceLabel}</dt>
        <dd>{props.model.balanceValue}</dd>
      </div>
    </dl>
  )
}

// CredentialUsageQuotaPanel frames adapted quota content in the shared subtle usage surface.
// CredentialUsageQuotaPanel 使用共享的浅色用量底板承载已适配额度内容。
// Parameters: props.children is the quota content and props.header an optional account-facts row above it;
// a panel without quota content shows only the header row. Returns the framed panel.
// 参数：props.children 为额度内容，props.header 为其上方可选的账号事实行；没有额度内容时只显示头部行。返回带底板的面板。
export function CredentialUsageQuotaPanel(props: { children?: React.ReactNode; header?: React.ReactNode }) {
  // hasContent separates balance-only accounts, whose panel is just the facts row.
  // hasContent 区分仅余额账号，其面板只有事实行。
  const hasContent = props.children !== null && props.children !== undefined && props.children !== false
  return (
    <div className="vk-usage-quota" data-slot="credential-usage">
      {props.header ? (
        <div data-ruled={hasContent ? "" : undefined} data-slot="credential-usage-header">
          {props.header}
        </div>
      ) : null}
      {hasContent ? (
        <div data-slot="credential-usage-content">
          {props.children}
        </div>
      ) : null}
    </div>
  )
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
  // usageRequiresManagementKey is projected from the manifest-selected usage primitive.
  // usageRequiresManagementKey 由清单选择的用量原语投影得到。
  usageRequiresManagementKey: boolean
  // usage is the latest exact account observation.
  // usage 是最新精确账号观测。
  usage?: PublicAiAccountUsage
}) {
  // t resolves provider-aware usage states in the active locale.
  // t 使用当前语言解析供应商感知的用量状态。
  const { t } = useI18n()
  if (props.credentialProfile?.account_usage_query === "unsupported") {
    return <span className="vk-usage-hint">{t("composer.accountCard.usageUnsupported")}</span>
  }
  if (props.usageRequiresManagementKey && !props.hasManagementKey) {
    return <span className="vk-usage-hint">{t("composer.accountCard.managementKeyRequired")}</span>
  }
  if (!props.usage) {
    return <span className="vk-usage-hint">{t("composer.accountCard.usageNotQueried")}</span>
  }
  if (props.usage.availability === "temporarily_unavailable") {
    return (
      <span className="vk-usage-hint">
        {props.usage.refresh_message ?? t("composer.accountCard.usageUnavailable")}
      </span>
    )
  }
  if (!Array.isArray(props.usage.quota_windows)) {
    return <span className="vk-usage-hint" data-tone="bad">{t("composer.accountCard.usageInvalid")}</span>
  }
  // rows joins MiniMax rolling and weekly windows into resource buckets.
  // rows 将 MiniMax 滚动周期与周周期组合为资源桶。
  const rows = props.credentialProfile?.account_usage_query === "mini_max_token_plan"
    ? miniMaxUsageRows(props.usage.quota_windows)
    : []
  if (rows.length > 0) {
    return <MiniMaxUsageSummary rows={rows} />
  }
  // commandCodeRows selects the three stable credit windows for the compact CommandCode layout.
  // commandCodeRows 为紧凑 CommandCode 布局选择三个稳定的额度窗口。
  const commandCodeRows = props.credentialProfile?.account_usage_query === "command_code_usage"
    ? commandCodeUsageRows(props.usage.quota_windows)
    : null
  if (commandCodeRows) {
    return <CommandCodeUsageSummary rows={commandCodeRows} />
  }
  // tavilyPlanWindow selects only the account-plan allowance used by the compact Tavily card.
  // tavilyPlanWindow 仅选择 Tavily 紧凑卡片使用的账号套餐额度。
  const tavilyPlanWindow = props.credentialProfile?.account_usage_query === "tavily_credits"
    ? props.usage.quota_windows.find((window) => window.id === "tavily-account-plan")
    : undefined
  if (props.credentialProfile?.account_usage_query === "tavily_credits") {
    return <TavilyUsageSummary window={tavilyPlanWindow} />
  }
  if (props.usage.quota_windows.length === 0) {
    return <span className="vk-usage-hint">{t(
      props.credentialProfile?.account_usage_query === "opencode_go"
        ? "composer.accountCard.opencodeNoActiveQuota"
        : "composer.accountCard.noQuotaWindows",
    )}</span>
  }
  if (props.credentialProfile?.account_usage_query === "open_design_wallet") {
    return <OpenDesignUsageSummary windows={props.usage.quota_windows} />
  }
  if (props.credentialProfile?.account_usage_query === "open_ai_codex_usage") {
    return <CodexUsageSummary windows={props.usage.quota_windows} />
  }
  return (
    <div className="vk-usage-windows">
      {props.usage.quota_windows.slice(0, 3).map((window) => (
        <UsageWindowMetric
          key={window.id}
          label={props.credentialProfile?.account_usage_query === "kimi_coding_plan"
            ? kimiUsageWindowLabel(window, t)
            : props.credentialProfile?.account_usage_query === "command_code_usage"
              ? commandCodeUsageWindowLabel(window, t)
              : props.credentialProfile?.account_usage_query === "opencode_go"
                ? opencodeUsageWindowLabel(window, t)
                : window.display_name}
          window={window}
          showReset={props.credentialProfile?.account_usage_query === "opencode_go"}
        />
      ))}
    </div>
  )
}

// Resolves OpenCode's closed quota-window identities through the shared language catalog.
// 通过共享语言目录解析 OpenCode 封闭的额度窗口身份。
// window supplies one normalized meter and t resolves catalog keys; returns a period or invalid-data label.
// window 提供一个规范化窗口，t 解析目录键；返回周期或无效数据标签。
export function opencodeUsageWindowLabel(
  window: PublicAiAccountUsageWindow,
  t: (key: TranslationKey) => string,
): string {
  switch (window.id) {
    case "five-hour": return t("composer.accountCard.opencodeFiveHourQuota")
    case "week": return t("composer.accountCard.opencodeWeeklyQuota")
    case "month": return t("composer.accountCard.opencodeMonthlyQuota")
    default: return t("composer.accountCard.usageInvalid")
  }
}

// Localizes a provider-reported duration; window supplies exact metadata and t resolves catalog keys.
// 本地化供应商报告的周期；window 提供精确元数据，t 解析目录键。
// Returns a Design Plan period label, or the explicit invalid-data label for malformed metadata.
// 返回 Design Plan 周期名称；元数据非法时返回明确的无效数据标签。
export function openDesignUsageWindowLabel(
  window: PublicAiAccountUsageWindow,
  t: (key: TranslationKey) => string,
): string {
  // duration must be supplied by the verified preflight, never inferred from timestamps or policy names.
  // duration 必须由已验证预检提供，绝不从时间戳或策略名称推断。
  const duration = window.duration_seconds
  if (duration === undefined || !Number.isSafeInteger(duration) || duration <= 0) {
    return t("composer.accountCard.usageInvalid")
  }
  // Calendar labels derive from whole periods, while arbitrary valid periods retain their seconds.
  // 日期单位标签由完整周期派生，其他合法周期保留秒数。
  const secondsPerHour = 60 * 60
  const secondsPerDay = 24 * secondsPerHour
  if (duration % secondsPerDay === 0) {
    return t("composer.accountCard.openDesignDays").replace("{count}", String(duration / secondsPerDay))
  }
  if (duration % secondsPerHour === 0) {
    return t("composer.accountCard.openDesignHours").replace("{count}", String(duration / secondsPerHour))
  }
  return t("composer.accountCard.openDesignSeconds").replace("{count}", String(duration))
}

// Renders independent plan windows from props.windows; returns percentage bars and server-owned reset times.
// 根据 props.windows 渲染独立套餐窗口；返回百分比进度条及服务端重置时间。
function OpenDesignUsageSummary(props: { windows: PublicAiAccountUsageWindow[] }) {
  // t resolves period names without embedding a second provider or language catalog.
  // t 解析周期名称，不内嵌第二份供应商或语言目录。
  const { t } = useI18n()
  // The mounted card refreshes minute-resolution countdowns even when account usage is unchanged.
  // 已挂载卡片在账户用量未变化时也刷新分钟精度的倒计时。
  const [, setClock] = React.useState(Date.now)
  React.useEffect(() => {
    // interval is scoped to this visible summary and is removed when its card closes.
    // interval 作用于当前可见摘要，卡片关闭后即移除。
    const interval = window.setInterval(() => setClock(Date.now()), 60_000)
    return () => window.clearInterval(interval)
  }, [])
  return (
    <div className="vk-usage-windows" data-slot="open-design-plan-usage">
      {props.windows.map((window) => (
        <UsageWindowMetric
          key={window.id}
          label={openDesignUsageWindowLabel(window, t)}
          window={window}
          showRemainingPercentage
          showReset
        />
      ))}
    </div>
  )
}

// codexUsageWindowLabel names a Codex window by its provider-reported period and optional limit bucket.
// codexUsageWindowLabel 按供应商报告的周期及可选限额桶命名一个 Codex 窗口。
// window supplies exact metadata and t resolves catalog keys; returns the period label, prefixed by
// the bucket name for model-specific limits, or the provider display name when the period is absent.
// window 提供精确元数据，t 解析目录键；返回周期标签，模型专属限额前置桶名称，周期缺失时返回供应商显示名称。
export function codexUsageWindowLabel(
  window: PublicAiAccountUsageWindow,
  t: (key: TranslationKey) => string,
): string {
  const duration = window.duration_seconds
  const secondsPerHour = 60 * 60
  const secondsPerDay = 24 * secondsPerHour
  let period: string | undefined
  if (duration !== undefined && Number.isSafeInteger(duration) && duration > 0) {
    if (duration === 7 * secondsPerDay) {
      period = t("composer.accountCard.codexWeeklyWindow")
    } else if (duration % secondsPerDay === 0) {
      period = t("composer.accountCard.codexDaysWindow").replace("{count}", String(duration / secondsPerDay))
    } else if (duration % secondsPerHour === 0) {
      period = t("composer.accountCard.codexHoursWindow").replace("{count}", String(duration / secondsPerHour))
    }
  }
  if (!period) {
    return window.display_name
  }
  return window.bucket_name ? `${window.bucket_name} · ${period}` : period
}

// Renders every Codex window from props.windows as a remaining-percentage bar with its reset countdown.
// 将 props.windows 中每个 Codex 窗口渲染为带重置倒计时的剩余百分比进度条。
function CodexUsageSummary(props: { windows: PublicAiAccountUsageWindow[] }) {
  // t resolves period names from the shared language catalog.
  // t 从共享语言目录解析周期名称。
  const { t } = useI18n()
  return (
    <div className="vk-usage-windows" data-slot="codex-usage">
      {props.windows.map((window) => (
        <UsageWindowMetric
          key={window.id}
          label={codexUsageWindowLabel(window, t)}
          window={window}
          showRemainingPercentage
          showReset
        />
      ))}
    </div>
  )
}

// kimiUsageWindowLabel translates ModelCore-compatible Kimi metrics while preserving provider-owned names.
// kimiUsageWindowLabel 翻译与 ModelCore 兼容的 Kimi 指标，同时保留供应商拥有的名称。
export function kimiUsageWindowLabel(
  window: PublicAiAccountUsageWindow,
  t: (key: TranslationKey) => string,
): string {
  if (window.id === "weekly_usage") {
    return t("composer.accountCard.kimiWeeklyUsage")
  }
  if (window.id === "five_hour_usage" || window.id === "5_hour_usage") {
    return t("composer.accountCard.kimiFiveHourUsage")
  }
  const fallbackMatch = /^limit_(\d+)$/.exec(window.id)
  if (fallbackMatch) {
    return `${t("composer.accountCard.kimiLimit")} ${fallbackMatch[1]}`
  }
  return window.display_name
}

// commandCodeUsageWindowLabel localizes stable CommandCode quota-window identities.
// commandCodeUsageWindowLabel 本地化稳定的 CommandCode 额度窗口身份。
//
// Parameters
// 参数
// `window` is one normalized CommandCode credit or request window and `t` resolves active-locale text.
// `window` 是一个规范化的 CommandCode 额度或请求窗口，`t` 解析当前语言文案。
//
// Returns
// 返回值
// A localized known label or the provider-owned display name for future unknown windows.
// 返回已知窗口的本地化标签；未来未知窗口则返回供应商拥有的显示名称。
export function commandCodeUsageWindowLabel(
  window: PublicAiAccountUsageWindow,
  t: (key: TranslationKey) => string,
): string {
  const labels: Partial<Record<string, TranslationKey>> = {
    "monthly-credits": "composer.accountCard.commandCodeMonthlyCredits",
    "purchased-credits": "composer.accountCard.commandCodePurchasedCredits",
    "free-credits": "composer.accountCard.commandCodeFreeCredits",
    "five-hour-requests": "composer.accountCard.commandCodeFiveHourRequests",
    "weekly-requests": "composer.accountCard.commandCodeWeeklyRequests",
  }
  const label = labels[window.id]
  return label ? t(label) : window.display_name
}

// CommandCodeUsageRows contains the exact credit windows used by the compact CommandCode card.
// CommandCodeUsageRows 包含紧凑 CommandCode 卡片使用的精确额度窗口。
interface CommandCodeUsageRows {
  // monthly is the full-width monthly credit window.
  // monthly 是占满整行的每月额度窗口。
  monthly?: PublicAiAccountUsageWindow
  // purchased is the first compact window on the second row.
  // purchased 是第二行的第一个紧凑窗口。
  purchased?: PublicAiAccountUsageWindow
  // free is the second compact window on the second row.
  // free 是第二行的第二个紧凑窗口。
  free?: PublicAiAccountUsageWindow
}

// commandCodeUsageRows selects only the stable CommandCode credit windows without inventing missing data.
// commandCodeUsageRows 仅选择稳定的 CommandCode 额度窗口，不虚构缺失数据。
//
// Parameters
// 参数
// `windows` is the normalized account-usage window list returned by the provider adapter.
// `windows` 是供应商适配器返回的规范化账号用量窗口列表。
//
// Returns
// 返回值
// The recognized credit windows, or null when this response contains no CommandCode credit window.
// 已识别的额度窗口；当响应不包含 CommandCode 额度窗口时返回 null。
export function commandCodeUsageRows(
  windows: PublicAiAccountUsageWindow[],
): CommandCodeUsageRows | null {
  // windowsById indexes provider windows by their stable contract identifiers.
  // windowsById 按稳定契约标识为供应商窗口建立索引。
  const windowsById = new Map(windows.map((window) => [window.id, window]))
  // rows keeps the three display positions separate from provider response order.
  // rows 将三个展示位置与供应商响应顺序分离。
  const rows: CommandCodeUsageRows = {
    monthly: windowsById.get("monthly-credits"),
    purchased: windowsById.get("purchased-credits"),
    free: windowsById.get("free-credits"),
  }
  return rows.monthly || rows.purchased || rows.free ? rows : null
}

// CommandCodeUsageSummary renders monthly credits on one row and the other credit buckets side by side.
// CommandCodeUsageSummary 将每月额度单独占一行，并将其余额度桶并列展示。
function CommandCodeUsageSummary(props: { rows: CommandCodeUsageRows }) {
  // t resolves the stable CommandCode credit labels in the active locale.
  // t 使用当前语言解析稳定的 CommandCode 额度标签。
  const { t } = useI18n()
  return (
    <div className="vk-usage-windows">
      {props.rows.monthly ? (
        <UsageWindowMetric
          label={commandCodeUsageWindowLabel(props.rows.monthly, t)}
          window={props.rows.monthly}
        />
      ) : null}
      {props.rows.purchased || props.rows.free ? (
        <div className="vk-usage-pair" data-slot="credential-credit-balances">
          {props.rows.purchased ? (
            <UsageWindowMetric
              label={commandCodeUsageWindowLabel(props.rows.purchased, t)}
              window={props.rows.purchased}
            />
          ) : null}
          {props.rows.free ? (
            <UsageWindowMetric
              label={commandCodeUsageWindowLabel(props.rows.free, t)}
              window={props.rows.free}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

// TavilyUsageSummary renders only provider-reported account-plan consumption.
// TavilyUsageSummary 仅渲染供应商报告的账号套餐消费。
// Parameters: props supplies provider-reported plan consumption; returns the corresponding usage summary without estimating absent counters.
// 参数：props 提供供应商报告的套餐消耗；返回对应用量摘要，不估算缺失计数。
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
    <div className="vk-usage-windows">
      <div className="vk-usage-row" data-tone={percentage === null ? undefined : usageMeterTone(percentage)}>
        <strong className="vk-usage-name">{t("composer.accountCard.tavilyPlanQuota")}</strong>
        <span className="vk-usage-value">
          {t("composer.accountCard.used")} {value}
        </span>
        {hasFiniteLimit && percentage !== null ? (
          <UsageMeter
            label={`${t("composer.accountCard.tavilyPlanQuota")}: ${Math.round(percentage)}%`}
            percentage={percentage}
            tone={usageMeterTone(percentage)}
          />
        ) : null}
      </div>
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
    <div className="vk-usage-windows">
      {props.rows.map((row) => (
        <div
          className="vk-usage-bucket"
          data-slot="credential-usage-bucket-row"
          key={row.name}
        >
          <span>{miniMaxBucketLabel(row.name, t)}</span>
          <UsageWindowMetric label={t("composer.accountCard.remaining")} window={row.current} />
          <UsageWindowMetric label={t("composer.accountCard.weeklyRemaining")} window={row.weekly} />
          <span>
            {t("composer.accountCard.reset")} {usageResetDuration(row.current?.resets_at)}
          </span>
        </div>
      ))}
    </div>
  )
}

// UsageWindowMetric renders one exact remaining value and compact progress indicator.
// UsageWindowMetric 渲染一个精确剩余值和紧凑进度指示器。
// Parameters: props supplies one quota window and display context; returns its exact remaining value and a progress bar only when determined.
// 参数：props 提供一个额度窗口与显示上下文；返回精确剩余值，仅在数据确定时渲染进度条。
function UsageWindowMetric(props: {
  // label names the quota window.
  // label 命名额度周期。
  label: string
  // window is the exact normalized provider observation.
  // window 是精确规范化供应商观测。
  window?: PublicAiAccountUsageWindow
  // showRemainingPercentage presents large credit pools as a readable percentage instead of raw counters.
  // showRemainingPercentage 将大型积分池显示为易读百分比而非原始计数。
  showRemainingPercentage?: boolean
  // showReset includes the provider-owned reset time beneath this independent window.
  // showReset 在独立窗口下方显示供应商持有的重置时间。
  showReset?: boolean
}) {
  // t resolves quota value labels in the active locale.
  // t 使用当前语言解析额度值标签。
  const { t } = useI18n()
  // percentage is the normalized progress value for the visual bar.
  // percentage 是用于视觉进度条的规范化进度值。
  const percentage = usageWindowPercentage(props.window)
  // value preserves the provider's exact quota semantics.
  // value 保留供应商的精确额度语义。
  const value = props.showRemainingPercentage && percentage !== null && props.window?.available
    ? `${t("composer.accountCard.remaining")} ${percentage}%`
    : usageWindowValue(props.window, t)
  return (
    <div className="vk-usage-row" data-tone={percentage === null ? undefined : usageMeterTone(percentage)}>
      <span className="vk-usage-name">{props.label}</span>
      <span className="vk-usage-value">{value}</span>
      {percentage !== null ? (
        <UsageMeter label={`${props.label}: ${value}`} percentage={percentage} tone={usageMeterTone(percentage)} />
      ) : null}
      {props.showReset && props.window?.resets_at ? (
        <span className="vk-usage-reset" data-slot="quota-window-reset">
          {t("composer.accountCard.reset")} {usageResetDuration(props.window.resets_at)}
        </span>
      ) : null}
    </div>
  )
}

// USAGE_CRITICAL_PERCENT is the remaining share at or below which a quota counts as nearly exhausted.
// USAGE_CRITICAL_PERCENT 是额度视为即将耗尽的剩余比例上限（含）。
export const USAGE_CRITICAL_PERCENT = 10
// USAGE_WARNING_PERCENT is the remaining share at or below which a quota is shown as running low.
// USAGE_WARNING_PERCENT 是额度显示为偏低的剩余比例上限（含）。
const USAGE_WARNING_PERCENT = 30

// usageMeterTone grades a remaining-quota share by the shared thresholds.
// usageMeterTone 按共享阈值为剩余额度比例分档。
export function usageMeterTone(percentage: number): UsageMeterTone {
  if (percentage <= USAGE_CRITICAL_PERCENT) return "bad"
  if (percentage <= USAGE_WARNING_PERCENT) return "warn"
  return "ok"
}

// usageWindowPercentage returns the normalized visual percentage without inventing quota.
// usageWindowPercentage 返回规范化视觉百分比且不虚构额度。
export function usageWindowPercentage(window?: PublicAiAccountUsageWindow): number | null {
  if (!window || !window.available) {
    return null
  }
  if (window.unlimited) {
    return 100
  }
  // explicit is the provider-reported remaining percentage.
  // explicit 是供应商报告的剩余百分比。
  const explicit = Number(window.remaining_percent)
  if (window.remaining_percent !== undefined && window.remaining_percent.trim() !== "" && Number.isFinite(explicit)) {
    return Math.max(0, Math.min(100, explicit))
  }
  // Integer credit pools can exceed Number's range; divide exactly before converting a bounded percentage.
  // 整数积分池可能超过 Number 范围；先精确相除，再转换有界百分比。
  if (window.remaining !== undefined && window.total !== undefined
    && /^\d+$/.test(window.remaining) && /^\d+$/.test(window.total)) {
    // The hundredth-percent ratio stays exact even for counters larger than a fixed-width integer.
    // 即使计数超过固定宽度整数，百分之一百分点比例仍保持精确。
    const total = BigInt(window.total)
    if (total === 0n) return null
    const basisPoints = (BigInt(window.remaining) * 10_000n) / total
    return Number(basisPoints > 10_000n ? 10_000n : basisPoints) / 100
  }
  // remaining and total preserve exact count-based quota semantics.
  // remaining 与 total 保留基于次数的精确额度语义。
  const remaining = Number(window.remaining)
  const total = Number(window.total)
  if (window.remaining !== undefined && window.remaining.trim() !== "" && Number.isFinite(remaining) && Number.isFinite(total) && total > 0) {
    return Math.max(0, Math.min(100, (remaining / total) * 100))
  }
  return null
}

// usageWindowValue renders exact counts, percentage, unlimited, or unavailable state.
// usageWindowValue 渲染精确次数、百分比、无限或不可用状态。
// Parameters: window contains authoritative quota fields and t resolves text; returns known count, percentage, unlimited or unavailable copy.
// 参数：window 包含权威额度字段，t 解析文案；返回已知次数、百分比、无限或不可用文案。
export function usageWindowValue(
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
  if (window.total !== undefined && window.total.trim() !== "" && Number.isFinite(total) && total >= 0) {
    if (window.unit === "USD") {
      return `${window.remaining === undefined ? "-" : formatCurrencyAmount(window.remaining)} / ${formatCurrencyAmount(window.total)} USD`
    }
    return `${window.remaining ?? "-"} / ${window.total}`
  }
  if (window.remaining !== undefined && window.remaining.trim() !== "") return window.unit === "USD" ? `${formatCurrencyAmount(window.remaining)} USD` : window.remaining
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
// Parameters: usage supplies the account balance and currency fields; returns their display label without numeric coercion.
// 参数：usage 提供账号余额与币种字段；返回不经数值强制转换的展示标签。
export function accountBalanceLabel(usage?: PublicAiAccountUsage): string {
  if (!usage?.balance_amount) {
    return "-"
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
