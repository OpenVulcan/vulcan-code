import * as React from "react"
import { ExternalLink, Info, KeyRound, ListTree, MoreHorizontal, Pencil, Stethoscope, Trash2, TriangleAlert, WandSparkles } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { protocolDisplayName } from "@/components/features/settings/provider-management-settings/protocol-display-name"
import { ProviderModelCatalog } from "@/components/features/settings/provider-model-settings"
import { isFunctionalProvider } from "@/components/features/settings/provider-ordering"
import { localizedProviderDescription, localizedProviderName } from "@/components/features/settings/provider-presentation"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu"
import { useI18n } from "@/i18n"
import type {
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

import { ProviderAccounts } from "./provider-accounts"
import type { ProviderAlert } from "./provider-alerts"
import { ConnectProgress, ProviderConnectPanel } from "./provider-connect"
import { AffiliateChip, providerCategoryLabel, providerSignupUrl, ProviderTagChips } from "./provider-intro"
import type { SnapshotMutations } from "./use-snapshot-mutations"
import "@/ui-kit/styles/features/settings-forms.css"

// ProviderWorkspaceProps binds the selected provider to the page-owned state and dialogs.
// ProviderWorkspaceProps 将选中的供应商绑定到页面持有的状态与弹窗。
export interface ProviderWorkspaceProps {
  snapshot: PublicAiSettingsSnapshot
  provider: PublicAiProviderDescriptor
  mutations: SnapshotMutations
  onSnapshot: (next: PublicAiSettingsSnapshot) => void
  usageByConnection: Readonly<Record<string, PublicAiAccountUsage>>
  onUsage: (connectionId: string, usage: PublicAiAccountUsage) => void
  alerts: readonly ProviderAlert[]
  // viewSwitch is the page's provider/model switch, placed in this header.
  // viewSwitch 是页面的供应商/模型切换，放在此页头中。
  viewSwitch: React.ReactNode
  // connectingId is the account just added, whose connect progress is shown.
  // connectingId 是刚添加的账号，显示其接入进度。
  connectingId: string | null
  onDismissConnecting: () => void
  onAddAccount: (providerId: string, entryId?: string) => void
  onManageDirectory: (provider: PublicAiProviderDescriptor) => void
  onEditCustom: (provider: PublicAiProviderDescriptor) => void
  onRemoveCustom: (provider: PublicAiProviderDescriptor) => void
  onOpenFunctions: () => void
  onOpenDiagnostics: () => void
}

// modelAccounts lists the provider's accounts that own a model catalog.
// modelAccounts 列出该供应商拥有模型目录的账号。
function modelAccounts(snapshot: PublicAiSettingsSnapshot, provider: PublicAiProviderDescriptor): PublicAiConnection[] {
  return snapshot.connections.filter((connection) => {
    if (connection.provider_id !== provider.id) return false
    const entry = provider.service_entries.find((candidate) => candidate.id === connection.service_entry_id)
    return entry?.kind !== "provider_tools"
  })
}

// ProviderWorkspace is the right pane: who the provider is, its accounts, then its models.
// A provider without accounts shows its introduction and entry choice instead.
// ProviderWorkspace 是右侧面板：供应商是谁、其账号，然后是其模型。
// 尚无账号的供应商改为显示介绍与接入方式选择。
export function ProviderWorkspace(props: ProviderWorkspaceProps) {
  const { t } = useI18n()
  const { snapshot, provider, mutations } = props
  const functional = isFunctionalProvider(provider)
  const busy = mutations.pendingKey !== null
  const accounts = modelAccounts(snapshot, provider)
  const accountCount = snapshot.connections.filter((connection) => connection.provider_id === provider.id).length
  // scopeId narrows the model list to one account; it resets when the provider changes.
  // scopeId 将模型列表收窄到一个账号；切换供应商时重置。
  const [scopeId, setScopeId] = React.useState<string | null>(null)
  const [showDetails, setShowDetails] = React.useState(false)
  React.useEffect(() => {
    setScopeId(null)
    setShowDetails(false)
  }, [provider.id])
  const scope = accounts.find((connection) => connection.id === scopeId) ?? null
  const connecting = props.connectingId
    ? snapshot.connections.find((connection) => connection.id === props.connectingId && connection.provider_id === provider.id) ?? null
    : null

  return (
    <div className="vk-ms-workspace">
      <WorkspaceHeader {...props} onToggleDetails={() => setShowDetails((current) => !current)} showDetails={showDetails} />

      {accountCount === 0 ? (
        <ProviderConnectPanel busy={busy} onConnect={(entryId) => props.onAddAccount(provider.id, entryId)} provider={provider} />
      ) : (
        <>
          {connecting ? (
            <ConnectProgress
              connection={connecting}
              mutations={mutations}
              onAdjust={() => setScopeId(connecting.id)}
              onDismiss={props.onDismissConnecting}
              onUsage={props.onUsage}
              signupUrl={providerSignupUrl(provider)}
              snapshot={snapshot}
              usage={props.usageByConnection[connecting.id]}
            />
          ) : null}
          {functional || (provider.service_tags ?? []).some((tag) => tag !== "model") ? (
            <p className="vk-ms-note">
              <WandSparkles aria-hidden="true" className="vk-icon" />
              <span>{t(functional ? "settings.modelServices.functional.only" : "settings.modelServices.functional.also")}</span>
              <Button onClick={props.onOpenFunctions} size="xs" type="button" variant="link">{t("settings.modelServices.functional.open")}</Button>
            </p>
          ) : null}
          <section className="vk-ms-section">
            <h3 className="vk-ms-section-title">
              {t("settings.modelServices.section.accounts")}<small>{accountCount}</small>
            </h3>
            <ProviderAccounts
              alerts={props.alerts}
              mutations={mutations}
              onScope={setScopeId}
              onSnapshot={props.onSnapshot}
              provider={provider}
              scopeId={scope?.id ?? null}
              snapshot={snapshot}
              usageByConnection={props.usageByConnection}
            />
          </section>
          {!functional ? (
            <section className="vk-ms-section vk-ms-models">
              <div className="vk-ms-section-bar">
                <h3 className="vk-ms-section-title">{t("settings.modelServices.section.models")}</h3>
                <Button disabled={busy} onClick={() => props.onManageDirectory(provider)} size="xs" type="button" variant="outline">
                  <ListTree aria-hidden="true" />{t("settings.customProviders.manageModels")}
                </Button>
              </div>
              {accounts.length > 0 ? (
                <div aria-label={t("settings.modelServices.scope.label")} className="vk-ms-scope" role="group">
                  <span>{t("settings.modelServices.scope.label")}</span>
                  <button aria-pressed={scope === null} className="vk-ms-tag-chip" onClick={() => setScopeId(null)} type="button">
                    {t("settings.modelServices.scope.all")}
                  </button>
                  {accounts.map((connection) => (
                    <button
                      aria-pressed={scope?.id === connection.id}
                      className="vk-ms-tag-chip"
                      key={connection.id}
                      onClick={() => setScopeId(connection.id)}
                      title={t("settings.modelServices.scope.hint")}
                      type="button"
                    >{connection.display_name}</button>
                  ))}
                  <small>{t(scope ? "settings.modelServices.scope.accountHint" : "settings.modelServices.scope.allHint")}</small>
                </div>
              ) : null}
              <ProviderModelCatalog
                accountScope={scope}
                mutations={mutations}
                onSnapshot={props.onSnapshot}
                providerIds={[provider.id]}
                showProvider={false}
                snapshot={snapshot}
              />
            </section>
          ) : null}
        </>
      )}
    </div>
  )
}

// WorkspaceHeader names the provider and holds the view switch, the primary action and a menu for the rest.
// WorkspaceHeader 显示供应商名称，并放置视图切换、主要操作及收纳其余操作的菜单。
function WorkspaceHeader(props: ProviderWorkspaceProps & { showDetails: boolean; onToggleDetails: () => void }) {
  const { t } = useI18n()
  const { provider } = props
  const name = localizedProviderName(provider.id, provider.display_name, t)
  const category = providerCategoryLabel(provider.category, t)
  const signupUrl = providerSignupUrl(provider)
  const busy = props.mutations.pendingKey !== null
  const protocols = Array.from(new Set(provider.service_entries.flatMap(
    (entry) => entry.protocol_routes.map((route) => protocolDisplayName(route.protocol_profile_id)),
  )))
  return (
    <>
      <header className="vk-ms-head">
        <ProviderIcon category={provider.category} size={36} providerId={provider.id} />
        <div className="vk-ms-head-copy">
          <div className="vk-ms-head-title">
            <h2>{name}</h2>
            <ProviderTagChips provider={provider} />
            {category ? <span className="vk-ms-chip" data-variant="category">{category}</span> : null}
            {provider.kind === "custom" ? <span className="vk-ms-chip" data-variant="category">{t("settings.modelServices.category.custom")}</span> : null}
            <AffiliateChip provider={provider} />
            {provider.credential_directory_visibility === "existing_accounts_only" ? (
              <Badge variant="warning">
                <TriangleAlert aria-hidden="true" />{t("settings.modelServices.providerUnstable")}
              </Badge>
            ) : null}
          </div>
          <p>
            {localizedProviderDescription(provider.id, t)}
            {signupUrl ? (
              <a href={signupUrl} rel="noreferrer" target="_blank" title={provider.promotion ? t(provider.promotion.disclosure_key) : undefined}>
                {t("settings.modelServices.signup")}<ExternalLink aria-hidden="true" className="vk-icon" />
              </a>
            ) : null}
          </p>
        </div>
        <div className="vk-ms-head-actions">
          {props.viewSwitch}
          <Button disabled={busy} onClick={() => props.onAddAccount(provider.id)} size="sm" type="button">
            <KeyRound aria-hidden="true" />{t("settings.providerOnboarding.addCredential")}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button aria-label={t("settings.modelServices.more")} size="icon-sm" type="button" variant="outline"><MoreHorizontal /></Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={props.onToggleDetails}><Info />{t("settings.modelServices.head.details")}</DropdownMenuItem>
              <DropdownMenuItem onSelect={props.onOpenDiagnostics}><Stethoscope />{t("settings.modelServices.tabs.diagnostics")}</DropdownMenuItem>
              {provider.kind === "custom" ? (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem disabled={busy} onSelect={() => props.onEditCustom(provider)}>
                    <Pencil />{t("settings.providers.editFor").replace("{provider}", name)}
                  </DropdownMenuItem>
                  <DropdownMenuItem disabled={busy} onSelect={() => props.onRemoveCustom(provider)} variant="destructive">
                    <Trash2 />{t("settings.providers.removeFor").replace("{provider}", name)}
                  </DropdownMenuItem>
                </>
              ) : null}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </header>
      {props.showDetails ? (
        <dl className="vk-ms-head-facts">
          <div><dt>{t("settings.modelServices.head.id")}</dt><dd className="vk-mono">{provider.id}</dd></div>
          {protocols.length > 0 ? <div><dt>{t("settings.modelServices.head.protocols")}</dt><dd>{protocols.join(" · ")}</dd></div> : null}
          {provider.service_entries.length > 0 ? (
            <div><dt>{t("settings.modelServices.head.entries")}</dt><dd>{provider.service_entries.map((entry) => entry.display_name).join(" · ")}</dd></div>
          ) : null}
          {provider.kind === "custom" ? (
            <div><dt>{t("settings.providers.table.endpoint")}</dt><dd className="vk-mono vk-form-break">{provider.service_entries.map((entry) => entry.base_url).join(" · ")}</dd></div>
          ) : null}
        </dl>
      ) : null}
    </>
  )
}
