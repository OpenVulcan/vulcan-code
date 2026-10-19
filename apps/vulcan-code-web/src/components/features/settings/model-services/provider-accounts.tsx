import * as React from "react"
import { Boxes, ExternalLink, KeyRound, ListChecks, MoreHorizontal, Pencil, RefreshCw, Star, Trash2, UserRound } from "lucide-react"

import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context"
import { CredentialModelAccessDialog } from "@/components/features/settings/credential-model-access-dialog"
import {
  credentialSupportsModelSynchronization,
  PublicAiConnectionDialog,
  PublicAiDeleteConnectionDialog,
} from "@/components/features/settings/provider-list-settings"
import { localizedServiceEntryPresentation } from "@/components/features/settings/provider-presentation"
import { PageEmptyState } from "@/components/page"
import { credentialUsageIsNonPlanAccount, useCredentialUsageOverview } from "@/components/public-ai/credential-usage-overview"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import {
  queryPublicAiConnectionUsage,
  savePublicAiConnectionEnabled,
  savePublicAiDefaultConnection,
  syncPublicAiConnection,
} from "@/service/settings-actions-service"
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
        <div className="model-services-accounts">
          {connections.map((connection) => (
            <AccountCard
              alerts={props.alerts.filter((alert) => alert.connectionId === connection.id)}
              busy={mutations.pendingKey !== null}
              connection={connection}
              key={connection.id}
              onDelete={() => setDeleting(connection)}
              onEdit={() => setEditing(connection)}
              onEnabledChange={(enabled) => run(`account/${connection.id}`, () => savePublicAiConnectionEnabled({ connectionId: connection.id, enabled }))}
              onManageModels={() => setModelAccess(connection)}
              onRefreshUsage={() => run(`usage/${connection.id}`, async () => {
                props.onUsage(connection.id, await queryPublicAiConnectionUsage(connection.id))
              })}
              onScope={() => props.onScope(props.scopeId === connection.id ? null : connection.id)}
              onSetDefault={() => run(`account/${connection.id}`, () => savePublicAiDefaultConnection({
                providerId: connection.provider_id,
                connectionId: connection.id,
              }))}
              onSync={() => run(`account/${connection.id}`, () => syncPublicAiConnection(connection.id))}
              provider={provider}
              scoped={props.scopeId === connection.id}
              snapshot={snapshot}
              usage={props.usageByConnection[connection.id] ?? snapshot.usageCache[connection.id]}
            />
          ))}
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
  scoped: boolean
  onScope: () => void
  onEdit: () => void
  onDelete: () => void
  onManageModels: () => void
  onEnabledChange: (enabled: boolean) => void
  onRefreshUsage: () => void
  onSetDefault: () => void
  onSync: () => void
}

// AccountCard shows who the account is, what it has left, and whether it works, with actions behind one menu.
// AccountCard 显示账号身份、剩余用量与是否正常，操作收在一个菜单里。
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
  const health = healthOf(props.alerts, connection.enabled)
  const catalogError = props.alerts.find((alert) => alert.kind === "catalog_error")?.detail
  const modelCount = snapshot.catalogs[connection.id]?.models.length ?? 0
  return (
    <article className="model-services-account" data-state={health}>
      <header className="model-services-account-top">
        <span className="model-services-account-name">
          {isDefault ? <Star aria-label={t("settings.credentials.currentDefault")} className="model-services-account-default" role="img" /> : null}
          <strong title={connection.display_name}>{connection.display_name}</strong>
        </span>
        <AccountMenu {...props} isDefault={isDefault} profile={profile} toolsOnly={toolsOnly} />
      </header>
      <div className="model-services-chip-row">
        <span className="model-services-chip">{presentation.title}</span>
        {presentation.tags.map((tag) => <span className="model-services-chip" key={tag}>{tag}</span>)}
      </div>
      <AccountUsage connection={connection} profile={profile} signupUrl={providerSignupUrl(provider)} usage={props.usage} />
      {catalogError ? <p className="model-services-account-error" title={catalogError}>{catalogError}</p> : null}
      <footer className="model-services-account-foot">
        <span className="model-services-status" data-state={health}>
          <i aria-hidden="true" />
          {healthLabel(health, props.alerts, t)}
          {!toolsOnly && connection.enabled ? ` · ${t("settings.modelServices.connect.enabled").replace("{count}", String(modelCount))}` : null}
        </span>
        <span className="model-services-account-controls">
          {!toolsOnly ? (
            <Button aria-pressed={props.scoped} className="model-services-scope-button" onClick={props.onScope} size="xs" type="button" variant="ghost">
              {t("settings.modelServices.account.onlyThis")}
            </Button>
          ) : null}
          {provider.credential_directory_visibility !== "core_account" ? (
            <Switch
              aria-label={connection.enabled ? t("settings.credentials.disable") : t("settings.credentials.enable")}
              checked={connection.enabled}
              disabled={props.busy}
              onCheckedChange={props.onEnabledChange}
            />
          ) : null}
        </span>
      </footer>
    </article>
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

// AccountMenu holds the account actions that are not needed at a glance.
// AccountMenu 收纳无需一眼看到的账号操作。
function AccountMenu(props: AccountCardProps & { isDefault: boolean; profile: PublicAiCredentialProfile | undefined; toolsOnly: boolean }) {
  const { t } = useI18n()
  const account = React.useContext(OpenVulcanAccountContext)
  const { connection } = props
  const core = props.provider.credential_directory_visibility === "core_account"
  const usageSupported = props.profile?.account_usage_query !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key)
  const directoryKind = props.snapshot.connectionModelAccessPolicyRecords
    .find((record) => record.connectionId === connection.id)?.directoryKind
  const blocked = props.busy || !connection.enabled || connection.pending_validation
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={t("settings.modelServices.account.menu").replace("{account}", connection.display_name)}
          size="icon-xs"
          type="button"
          variant="ghost"
        ><MoreHorizontal /></Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {core ? (
          <DropdownMenuItem onSelect={() => account?.setOpen(true)}><UserRound />{t("openvulcan.account.manage")}</DropdownMenuItem>
        ) : (
          <DropdownMenuItem disabled={props.busy} onSelect={props.onEdit}><Pencil />{t("settings.credentials.edit")}</DropdownMenuItem>
        )}
        {usageSupported ? (
          <DropdownMenuItem disabled={props.busy || !connection.enabled} onSelect={props.onRefreshUsage}><RefreshCw />{t("settings.credentials.refreshUsage")}</DropdownMenuItem>
        ) : null}
        {credentialSupportsModelSynchronization(directoryKind) ? (
          <DropdownMenuItem disabled={blocked} onSelect={props.onSync}><Boxes />{t("settings.credentials.syncModels")}</DropdownMenuItem>
        ) : null}
        {!props.toolsOnly ? (
          <DropdownMenuItem disabled={blocked} onSelect={props.onManageModels}><ListChecks />{t("settings.credentialModels.manage")}</DropdownMenuItem>
        ) : null}
        {!props.isDefault ? (
          <DropdownMenuItem disabled={blocked} onSelect={props.onSetDefault}><Star />{t("settings.credentials.setDefault")}</DropdownMenuItem>
        ) : null}
        {!core ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={props.busy} onSelect={props.onDelete} variant="destructive"><Trash2 />{t("settings.credentials.remove")}</DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// AccountUsage is the card's usage slot: plan and balance facts, then quota windows or where to look instead.
// AccountUsage 是卡片的用量区：先列套餐与余额，再显示额度窗口或应去何处查看。
function AccountUsage(props: {
  connection: PublicAiConnection
  profile: PublicAiCredentialProfile | undefined
  usage: PublicAiAccountUsage | undefined
  signupUrl: string | undefined
}) {
  const { t } = useI18n()
  const overview = useCredentialUsageOverview({ connection: props.connection, credentialProfile: props.profile, usage: props.usage })
  const unsupported = props.profile?.account_usage_query === "unsupported"
  const balanceOnly = credentialUsageIsNonPlanAccount(props.profile)
  const { model } = overview
  return (
    <div className="model-services-usage">
      <div className="model-services-usage-facts">
        <span>{model.planLabel}<b>{model.planValue}</b></span>
        {!unsupported ? <span>{model.balanceLabel}<b>{model.balanceValue}</b></span> : null}
      </div>
      {unsupported ? (
        <p className="model-services-usage-note">
          {t("composer.accountCard.usageUnsupported")}
          {props.signupUrl ? (
            <a href={props.signupUrl} rel="noreferrer" target="_blank">
              {t("settings.modelServices.usage.officialSite")}
              <ExternalLink aria-hidden="true" className="size-3" />
            </a>
          ) : null}
        </p>
      ) : balanceOnly ? null : (
        <div className="model-services-usage-quota">{overview.quotaContent}</div>
      )}
    </div>
  )
}
