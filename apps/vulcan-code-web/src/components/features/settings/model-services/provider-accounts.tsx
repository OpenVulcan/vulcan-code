import * as React from "react"
import { ExternalLink, KeyRound, ListChecks, MoreHorizontal, Pencil, RefreshCw, Star, Trash2 } from "lucide-react"

import { CredentialModelAccessDialog } from "@/components/features/settings/credential-model-access-dialog"
import {
  PublicAiConnectionDialog,
  PublicAiDeleteConnectionDialog,
} from "@/components/features/settings/provider-list-settings"
import { isRetiredCodexSubscription, localizedServiceEntryPresentation } from "@/components/features/settings/provider-presentation"
import { PageEmptyState } from "@/components/page"
import { credentialUsageIsNonPlanAccount, useCredentialUsageOverview } from "@/components/public-ai/credential-usage-overview"
import { Button, IconButton } from "@/ui-kit/primitives/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu"
import { Switch } from "@/ui-kit/primitives/switch"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip"
import { useI18n } from "@/i18n"
import { errorMessage } from "@/lib/display-format"
import { fetchPublicAiModelCandidates, queryPublicAiConnectionUsage, savePublicAiConnectionEnabled } from "@/service/settings-actions-service"
import type {
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiCredentialProfile,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

import { healthOf, type HealthState, type ProviderAlert } from "./provider-alerts"
import { providerSignupUrl } from "./provider-intro"
import type { SnapshotMutations } from "./use-snapshot-mutations"

// ProviderAccountsProps binds one provider's accounts to the page-owned state.
// ProviderAccountsProps 将单个供应商的账号绑定到页面持有的状态。
export interface ProviderAccountsProps {
  snapshot: PublicAiSettingsSnapshot
  provider: PublicAiProviderDescriptor
  mutations: SnapshotMutations
  onSnapshot: (next: PublicAiSettingsSnapshot) => void
  // usageByConnection holds usage reads newer than the snapshot cache.
  // usageByConnection 保存比快照缓存更新的用量读取。
  usageByConnection: Readonly<Record<string, PublicAiAccountUsage>>
  // Publishes the refreshed observation for the exact connection to the page.
  // 将刷新得到的账号用量发布到页面的对应连接。
  onUsage: (connectionId: string, usage: PublicAiAccountUsage) => void
  alerts: readonly ProviderAlert[]
  // scopeId is the account the model list is narrowed to; onScope toggles it.
  // scopeId 是模型列表收窄到的账号；onScope 切换它。
  scopeId: string | null
  onScope: (connectionId: string | null) => void
}

// ProviderAccounts lays the provider's accounts out as compact cards and owns their dialogs.
// ProviderAccounts 将供应商账号排成紧凑卡片，并持有相关弹窗。
export function ProviderAccounts(props: ProviderAccountsProps) {
  const { t } = useI18n()
  const { snapshot, provider, mutations } = props
  const [editing, setEditing] = React.useState<PublicAiConnection | null>(null)
  const [deleting, setDeleting] = React.useState<PublicAiConnection | null>(null)
  const [modelAccess, setModelAccess] = React.useState<PublicAiConnection | null>(null)
  const connections = snapshot.connections.filter((connection) => connection.provider_id === provider.id)
  const run = (key: string, task: () => Promise<PublicAiSettingsSnapshot | void>) => void mutations.run(key, task)

  return (
    <>
      {connections.length > 0 ? (
        <div className="vk-ms-accounts">
          {connections.map((connection) => {
            // Share the pending key with the connection panel's automatic usage read.
            // 与连接面板的自动用量读取共用等待标识。
            const usageKey = `usage/${connection.id}`
            return (
              <AccountCard
                alerts={props.alerts.filter((alert) => alert.connectionId === connection.id)}
                busy={mutations.pendingKey !== null}
                connection={connection}
                key={connection.id}
                onDelete={() => setDeleting(connection)}
                onEdit={() => setEditing(connection)}
                onEnabledChange={(enabled) => run(`account/${connection.id}`, () => savePublicAiConnectionEnabled({ connectionId: connection.id, enabled }))}
                onManageModels={() => setModelAccess(connection)}
                onRefreshUsage={() => run(usageKey, async () => {
                  props.onUsage(connection.id, await queryPublicAiConnectionUsage(connection.id))
                })}
                onScope={() => props.onScope(props.scopeId === connection.id ? null : connection.id)}
                provider={provider}
                refreshingUsage={mutations.pendingKey === usageKey}
                scoped={props.scopeId === connection.id}
                snapshot={snapshot}
                usage={props.usageByConnection[connection.id] ?? snapshot.usageCache[connection.id]}
              />
            )
          })}
        </div>
      ) : (
        <PageEmptyState
          description={t("settings.credentials.emptyProviderDescription")}
          icon={<KeyRound />}
          title={t("settings.credentials.emptyProvider")}
        />
      )}
      <CredentialModelAccessDialog
        connection={modelAccess}
        entry={modelAccess ? provider.service_entries.find((entry) => entry.id === modelAccess.service_entry_id) : undefined}
        onOpenChange={(open) => { if (!open) setModelAccess(null) }}
        onSnapshotChange={props.onSnapshot}
        open={modelAccess !== null}
        provider={provider}
        snapshot={snapshot}
      />
      <PublicAiConnectionDialog
        connection={editing}
        initialProviderId=""
        lockedProviderId={editing?.provider_id ?? ""}
        onOpenChange={(open) => { if (!open) setEditing(null) }}
        onSaved={(next) => {
          props.onSnapshot(next)
          setEditing(null)
        }}
        open={editing !== null}
        providers={snapshot.providers}
      />
      <PublicAiDeleteConnectionDialog
        connection={deleting}
        connections={snapshot.connections}
        isProviderDefault={Boolean(deleting && snapshot.providerDefaults[deleting.provider_id] === deleting.id)}
        onOpenChange={(open) => { if (!open) setDeleting(null) }}
        onSaved={(next) => {
          props.onSnapshot(next)
          setDeleting(null)
        }}
        open={deleting !== null}
      />
    </>
  )
}

// AccountCardProps carries one account, its health and the guarded actions it offers.
// AccountCardProps 承载一个账号、其健康状态及其提供的受保护操作。
interface AccountCardProps {
  snapshot: PublicAiSettingsSnapshot
  provider: PublicAiProviderDescriptor
  connection: PublicAiConnection
  alerts: readonly ProviderAlert[]
  usage: PublicAiAccountUsage | undefined
  busy: boolean
  // Indicates an in-flight usage query for this account.
  // 表示此账号正在查询用量。
  refreshingUsage: boolean
  scoped: boolean
  onScope: () => void
  onEdit: () => void
  onDelete: () => void
  onManageModels: () => void
  // Queries the current account when the user presses its refresh shortcut.
  // 用户点击刷新快捷按钮时查询当前账号。
  onRefreshUsage: () => void
  onEnabledChange: (enabled: boolean) => void
}

// AccountCard renders an account's identity, usage and health, and binds its controls to the supplied callbacks.
// AccountCard 渲染账号的身份、用量和状态，并将控件绑定到传入的回调。
function AccountCard(props: AccountCardProps) {
  const { t } = useI18n()
  const { snapshot, provider, connection } = props
  const entry = provider.service_entries.find((candidate) => candidate.id === connection.service_entry_id)
  const profile = entry?.credential_profiles.find((candidate) => candidate.id === connection.credential_profile_id)
  const presentation = entry
    ? localizedServiceEntryPresentation(provider.id, entry, t)
    : { title: connection.service_entry_id, tags: [] as string[] }
  const toolsOnly = entry?.kind === "provider_tools"
  const isDefault = snapshot.providerDefaults[provider.id] === connection.id
  const retired = isRetiredCodexSubscription(provider.id, connection.service_entry_id)
  const health = retired ? "warn" : healthOf(props.alerts, connection.enabled)
  const catalogError = props.alerts.find((alert) => alert.kind === "catalog_error")?.detail
  const modelCount = snapshot.catalogs[connection.id]?.models.length ?? 0
  // Use the active connection's projected usage capability and required credential.
  // 使用生效连接投影的用量能力及所需凭据。
  const usageSupported = connection.usage_capability !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key)
  // The ChatGPT OAuth entry manages authorization on the provider's usage page.
  // ChatGPT OAuth 入口在供应商用量页面管理授权。
  const chatGptPlan = provider.id === "openai" && connection.service_entry_id === "chatgpt-plan"
  // Share the translated names between hover hints and accessible button labels.
  // 悬停提示与按钮的无障碍名称共用翻译文案。
  const editLabel = t("settings.credentials.edit")
  const manageLabel = t("settings.credentialModels.manage")
  const refreshUsageLabel = t("settings.credentials.refreshUsage")
  // Keep the plan's official usage link and model connection check in the same card.
  // 将套餐的官方用量入口与模型连接检查放在同一张卡片内。
  const usageActions = chatGptPlan ? (
    <ChatGptPlanUsageActions busy={props.busy} connection={connection} usageSupported={usageSupported} />
  ) : null
  return (
    <article className="vk-ms-account" data-state={health}>
      <header className="vk-ms-account-top">
        <span className="vk-ms-account-name">
          {isDefault ? <Star aria-label={t("settings.credentials.currentDefault")} className="vk-ms-account-default" role="img" /> : null}
          <strong title={connection.display_name}>{connection.display_name}</strong>
        </span>
        <AccountMenu busy={props.busy} connection={connection} onDelete={props.onDelete} />
      </header>
      <div className="vk-ms-chip-row">
        <span className="vk-ms-chip">{presentation.title}</span>
        {presentation.tags.map((tag) => <span className="vk-ms-chip" key={tag}>{tag}</span>)}
      </div>
      {retired
        ? <p className="vk-form-note" role="status">{t("settings.credentials.retiredCodexMigration")}</p>
        : (
          <AccountUsage
            actions={usageActions}
            connection={connection}
            profile={profile}
            signupUrl={providerSignupUrl(provider)}
            usage={props.usage}
          />
        )}
      {!retired && catalogError ? <p className="vk-ms-account-error" title={catalogError}>{catalogError}</p> : null}
      <footer className="vk-ms-account-foot">
        <span className="vk-ms-status" data-state={health}>
          <i aria-hidden="true" />
          {retired ? t("settings.credentials.retiredCodexStatus") : healthLabel(health, props.alerts, t)}
          {!toolsOnly && connection.enabled ? ` · ${t("settings.modelServices.connect.enabled").replace("{count}", String(modelCount))}` : null}
        </span>
        {!retired ? <div className="vk-ms-account-actions">
          <div className="vk-ms-account-shortcuts">
            {/* The wrapper keeps hover hints available while the button is disabled. */}
            {/* 按钮禁用时，外层元素仍可显示悬停提示。 */}
            <Tooltip>
              <TooltipTrigger asChild>
                <span>
                  <IconButton aria-label={editLabel} disabled={props.busy} onClick={props.onEdit} size="xs" type="button">
                    <Pencil aria-hidden="true" />
                  </IconButton>
                </span>
              </TooltipTrigger>
              <TooltipContent>{editLabel}</TooltipContent>
            </Tooltip>
            {!toolsOnly ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span>
                    <IconButton
                      aria-label={manageLabel}
                      disabled={props.busy || !connection.enabled || connection.pending_validation}
                      onClick={props.onManageModels}
                      size="xs"
                      type="button"
                    >
                      <ListChecks aria-hidden="true" />
                    </IconButton>
                  </span>
                </TooltipTrigger>
                <TooltipContent>{manageLabel}</TooltipContent>
              </Tooltip>
            ) : null}
            {usageSupported ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span>
                    <IconButton
                      aria-busy={props.refreshingUsage}
                      aria-label={refreshUsageLabel}
                      disabled={props.busy || !connection.enabled}
                      onClick={props.onRefreshUsage}
                      size="xs"
                      type="button"
                    >
                      <RefreshCw aria-hidden="true" className={props.refreshingUsage ? "vk-spin" : undefined} />
                    </IconButton>
                  </span>
                </TooltipTrigger>
                <TooltipContent>{refreshUsageLabel}</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
          <span className="vk-ms-account-controls">
            {!toolsOnly ? (
              <Button aria-pressed={props.scoped} className="vk-ms-scope-button" onClick={props.onScope} size="xs" type="button" variant="ghost">
                {t("settings.modelServices.account.onlyThis")}
              </Button>
            ) : null}
            <Switch
              aria-label={connection.enabled ? t("settings.credentials.disable") : t("settings.credentials.enable")}
              checked={connection.enabled}
              disabled={props.busy}
              onCheckedChange={props.onEnabledChange}
            />
          </span>
        </div> : null}
      </footer>
    </article>
  )
}

// Shows ChatGPT usage management and returns a read-only model connection check for the selected account.
// 显示 ChatGPT 用量管理入口，并提供对所选账号模型连接的只读检查。
// The connection owns the credential identity, busy tracks settings writes, and usageSupported controls whether an in-app quota path exists.
// connection 提供凭据身份，busy 表示设置正在写入，usageSupported 表示是否存在应用内额度查询入口。
function ChatGptPlanUsageActions({ busy, connection, usageSupported }: Pick<AccountCardProps, "busy" | "connection"> & {
  // Indicates that the effective credential can query quota in the app.
  // 表示生效凭据可以在应用内查询额度。
  usageSupported: boolean
}) {
  const { t } = useI18n()
  // Own the request until the card is removed, and reject duplicate clicks while it runs.
  // 在卡片移除前持有请求，并阻止请求期间重复点击。
  const pending = React.useRef<AbortController | null>(null)
  // Record the last remote check outcome for this card only.
  // 仅在当前卡片记录最近一次远端检查的结果。
  const [result, setResult] = React.useState<{ state: "checking" | "reachable" } | { state: "failed"; detail: string } | null>(null)
  React.useEffect(() => () => pending.current?.abort(), [])

  // Match the Core discovery gate for the current credential.
  // 遵循 Core 对当前凭据的发现准入条件。
  const disabled = busy || !connection.enabled || connection.pending_validation
  // Project the request lifetime into the check button's feedback.
  // 将请求运行状态投影到检查按钮的反馈。
  const checking = result?.state === "checking"
  // Checks the account's isolated remote catalog and records a result when the card still owns the request.
  // 检查账号的隔离远程目录，在卡片仍持有请求时记录结果。
  const checkConnection = async () => {
    if (disabled || pending.current) return
    // Scope cancellation and feedback to this specific provider request.
    // 将取消与反馈限定在本次供应商请求内。
    const controller = new AbortController()
    pending.current = controller
    setResult({ state: "checking" })
    try {
      // This reads GET /v1/models through Core without publishing the observed catalog.
      // 经 Core 读取 GET /v1/models，且不发布本次观测的目录。
      await fetchPublicAiModelCandidates(connection.id, controller.signal)
      if (!controller.signal.aborted) setResult({ state: "reachable" })
    } catch (reason) {
      if (!controller.signal.aborted) setResult({ state: "failed", detail: errorMessage(reason) })
    } finally {
      pending.current = null
    }
  }

  return (
    <>
      <div className="vk-ms-usage-note vk-ms-usage-actions">
        <a href="https://chatgpt.com/settings/usage" rel="noreferrer" target="_blank">
          {t("settings.credentials.chatGptPlanUsage")}
          <ExternalLink aria-hidden="true" className="vk-icon" />
        </a>
        {!usageSupported ? (
          <Button aria-busy={checking} disabled={disabled || checking} onClick={checkConnection} size="xs" type="button" variant="ghost">
            <RefreshCw aria-hidden="true" className={checking ? "vk-spin" : undefined} />
            {t("settings.credentials.checkChatGptPlanConnection")}
          </Button>
        ) : null}
      </div>
      {!usageSupported ? <p className="vk-ms-usage-note">{t("settings.credentials.chatGptPlanUsageOnWeb")}</p> : null}
      {!usageSupported && connection.enabled && result?.state === "reachable" ? (
        <p className="vk-ms-usage-note" data-slot="chatgpt-plan-connection-checked" role="status">
          {t("settings.credentials.chatGptPlanConnectionChecked")}
        </p>
      ) : null}
      {!usageSupported && connection.enabled && result?.state === "failed" ? (
        <p className="vk-ms-account-error" role="alert" title={result.detail}>
          {t("settings.credentials.chatGptPlanConnectionCheckFailed")} · {result.detail}
        </p>
      ) : null}
    </>
  )
}

// healthLabel words one health state, naming the first alert when the account needs attention.
// healthLabel 描述一种健康状态；账号需关注时说明第一个提醒。
function healthLabel(health: HealthState, alerts: readonly ProviderAlert[], t: ReturnType<typeof useI18n>["t"]): string {
  switch (health) {
    case "off": return t("settings.credentials.disabledStatus")
    case "ok": return t("settings.modelServices.account.statusOk")
    case "bad": return t("settings.modelServices.alerts.catalogError")
    case "warn": return alerts[0]?.kind === "quota_low"
      ? t("settings.modelServices.alerts.quotaLow")
      : t("settings.modelServices.alerts.pendingValidation")
  }
}

// AccountMenu renders the account's delete entry and delegates confirmation to the supplied callback.
// AccountMenu 渲染账号的删除入口，并将确认流程交给传入的回调。
function AccountMenu({ busy, connection, onDelete }: Pick<AccountCardProps, "busy" | "connection" | "onDelete">) {
  const { t } = useI18n()
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton
          aria-label={t("settings.modelServices.account.menu").replace("{account}", connection.display_name)}
          size="xs"
          type="button"
        ><MoreHorizontal aria-hidden="true" /></IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem disabled={busy} onSelect={onDelete} variant="destructive">
          <Trash2 aria-hidden="true" />{t("settings.credentials.remove")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// AccountUsage is the card's usage slot: plan and balance facts, then quota windows or where to look instead.
// AccountUsage 是卡片的用量区：先列套餐与余额，再显示额度窗口或应去何处查看。
function AccountUsage(props: {
  // Optional authorization and refresh actions owned by the account's service entry.
  // 账号服务入口提供的可选授权与刷新操作。
  actions: React.ReactNode
  connection: PublicAiConnection
  profile: PublicAiCredentialProfile | undefined
  usage: PublicAiAccountUsage | undefined
  signupUrl: string | undefined
}) {
  const { t } = useI18n()
  const overview = useCredentialUsageOverview({ connection: props.connection, credentialProfile: props.profile, usage: props.usage })
  // Read support from the same live account projection as the refresh shortcut.
  // 从与刷新快捷按钮相同的实时账号视图中获取支持状态。
  const unsupported = props.connection.usage_capability === "unsupported"
  const balanceOnly = credentialUsageIsNonPlanAccount(props.profile)
  const { model } = overview
  return (
    <div className="vk-ms-usage">
      <div className="vk-ms-usage-facts">
        <span>{model.planLabel}<b>{model.planValue}</b></span>
        {!unsupported ? <span>{model.balanceLabel}<b>{model.balanceValue}</b></span> : null}
      </div>
      {unsupported && !props.actions ? (
        <p className="vk-ms-usage-note">
          {t("composer.accountCard.usageUnsupported")}
          {props.signupUrl ? (
            <a href={props.signupUrl} rel="noreferrer" target="_blank">
              {t("settings.modelServices.usage.officialSite")}
              <ExternalLink aria-hidden="true" className="vk-icon" />
            </a>
          ) : null}
        </p>
      ) : null}
      {!unsupported && (!balanceOnly || props.usage?.availability === "temporarily_unavailable") ? (
        <div className="vk-ms-usage-quota">{overview.quotaContent}</div>
      ) : null}
      {props.actions}
    </div>
  )
}
