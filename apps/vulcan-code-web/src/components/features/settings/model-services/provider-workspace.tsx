import * as React from "react"
import {
  CheckCircle2,
  CircleAlert,
  CircleDashed,
  ExternalLink,
  KeyRound,
  ListTree,
  LoaderCircle,
  Pencil,
  Trash2,
  WandSparkles,
  X,
} from "lucide-react"

import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context"
import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { ProviderAccountsSection } from "@/components/features/settings/provider-list-settings"
import { protocolDisplayName } from "@/components/features/settings/provider-management-settings/protocol-display-name"
import { ProviderModelCatalog } from "@/components/features/settings/provider-model-settings"
import { isFunctionalProvider } from "@/components/features/settings/provider-ordering"
import { localizedProviderDescription, localizedProviderName } from "@/components/features/settings/provider-presentation"
import { Button } from "@/components/ui/button"
import { useI18n } from "@/i18n"
import { cn } from "@/lib/utils"
import { queryPublicAiConnectionUsage, syncPublicAiConnection } from "@/service/settings-actions-service"
import type {
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"

import { providerCategoryLabel, providerSignupUrl, serviceTagLabel } from "./provider-intro"
import type { SnapshotMutations } from "./use-snapshot-mutations"

// ProviderWorkspaceProps binds the selected provider to the page-owned state and dialogs.
// ProviderWorkspaceProps 将选中的供应商绑定到页面持有的状态与弹窗。
export interface ProviderWorkspaceProps {
  snapshot: PublicAiSettingsSnapshot
  provider: PublicAiProviderDescriptor
  mutations: SnapshotMutations
  onSnapshot: (next: PublicAiSettingsSnapshot) => void
  usageByConnection: Readonly<Record<string, PublicAiAccountUsage>>
  onUsage: (connectionId: string, usage: PublicAiAccountUsage) => void
  // connectingId is the account just added, whose connect progress is shown.
  // connectingId 是刚添加的账号，显示其接入进度。
  connectingId: string | null
  onDismissConnecting: () => void
  onAddAccount: (providerId: string) => void
  onManageDirectory: (provider: PublicAiProviderDescriptor) => void
  onEditCustom: (provider: PublicAiProviderDescriptor) => void
  onRemoveCustom: (provider: PublicAiProviderDescriptor) => void
  onOpenFunctions: () => void
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
// ProviderWorkspace 是右侧面板：供应商是谁、其账号，然后是其模型。
export function ProviderWorkspace(props: ProviderWorkspaceProps) {
  const { t } = useI18n()
  const account = React.useContext(OpenVulcanAccountContext)
  const { snapshot, provider, mutations } = props
  const name = localizedProviderName(provider.id, provider.display_name, t)
  const category = providerCategoryLabel(provider.category, t)
  const signupUrl = providerSignupUrl(provider)
  const functional = isFunctionalProvider(provider)
  const busy = mutations.pendingKey !== null
  const accounts = modelAccounts(snapshot, provider)
  const accountCount = snapshot.connections.filter((connection) => connection.provider_id === provider.id).length
  // scopeId narrows the model list to one account; it resets when the provider changes.
  // scopeId 将模型列表收窄到一个账号；切换供应商时重置。
  const [scopeId, setScopeId] = React.useState<string | null>(null)
  React.useEffect(() => setScopeId(null), [provider.id])
  const scope = accounts.find((connection) => connection.id === scopeId) ?? null
  const protocols = Array.from(new Set(provider.service_entries.flatMap(
    (entry) => entry.protocol_routes.map((route) => protocolDisplayName(route.protocol_profile_id)),
  )))
  const coreAccount = provider.credential_directory_visibility === "core_account"
  const connecting = props.connectingId
    ? snapshot.connections.find((connection) => connection.id === props.connectingId && connection.provider_id === provider.id) ?? null
    : null

  return (
    <div className="model-services-workspace">
      <header className="model-services-head">
        <ProviderIcon category={provider.category} className="size-10 shrink-0" providerId={provider.id} />
        <div className="model-services-head-copy">
          <div className="model-services-head-title">
            <h2>{name}</h2>
            {category ? <span className="model-services-chip" data-variant="category">{category}</span> : null}
            {(provider.service_tags ?? []).map((tag) => (
              <span className="model-services-chip" data-tag={tag} key={tag}>{serviceTagLabel(tag, t)}</span>
            ))}
            {provider.kind === "custom" ? <span className="model-services-chip">{t("settings.modelServices.category.custom")}</span> : null}
          </div>
          <p>{localizedProviderDescription(provider.id, t)}</p>
          <dl className="model-services-head-facts">
            <div><dt>{t("settings.modelServices.head.id")}</dt><dd className="font-mono">{provider.id}</dd></div>
            {protocols.length > 0 ? <div><dt>{t("settings.modelServices.head.protocols")}</dt><dd>{protocols.join(" · ")}</dd></div> : null}
            {provider.service_entries.length > 1 ? (
              <div><dt>{t("settings.modelServices.head.entries")}</dt><dd>{provider.service_entries.map((entry) => entry.display_name).join(" · ")}</dd></div>
            ) : null}
            {provider.kind === "custom" ? (
              <div><dt>{t("settings.providers.table.endpoint")}</dt><dd className="font-mono break-all">{provider.service_entries.map((entry) => entry.base_url).join(" · ")}</dd></div>
            ) : null}
          </dl>
          {signupUrl ? (
            <p className="model-services-head-signup">
              <a href={signupUrl} rel="noreferrer" target="_blank">
                <ExternalLink aria-hidden="true" className="size-3.5" />
                {t("settings.modelServices.signup")}
              </a>
              {provider.promotion ? <small>{t(provider.promotion.disclosure_key)}</small> : null}
            </p>
          ) : null}
        </div>
        <div className="model-services-head-actions">
          <Button
            disabled={busy}
            onClick={() => coreAccount ? account?.setOpen(true) : props.onAddAccount(provider.id)}
            size="sm"
            type="button"
          >
            <KeyRound aria-hidden="true" />
            {coreAccount ? t("openvulcan.account.manage") : t("settings.providerOnboarding.addCredential")}
          </Button>
          {!functional ? (
            <Button
              aria-label={t("settings.customProviders.manageModelsFor").replace("{provider}", name)}
              disabled={busy}
              onClick={() => props.onManageDirectory(provider)}
              size="sm"
              type="button"
              variant="outline"
            >
              <ListTree aria-hidden="true" />
              {t("settings.customProviders.manageModels")}
            </Button>
          ) : null}
          {provider.kind === "custom" ? (
            <>
              <Button
                aria-label={t("settings.providers.editFor").replace("{provider}", name)}
                disabled={busy}
                onClick={() => props.onEditCustom(provider)}
                size="icon-sm"
                type="button"
                variant="outline"
              ><Pencil /></Button>
              <Button
                aria-label={t("settings.providers.removeFor").replace("{provider}", name)}
                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                disabled={busy}
                onClick={() => props.onRemoveCustom(provider)}
                size="icon-sm"
                type="button"
                variant="ghost"
              ><Trash2 /></Button>
            </>
          ) : null}
        </div>
      </header>

      {connecting ? (
        <ConnectProgress
          connection={connecting}
          mutations={mutations}
          onAdjust={() => setScopeId(connecting.id)}
          onDismiss={props.onDismissConnecting}
          onUsage={props.onUsage}
          signupUrl={signupUrl}
          snapshot={snapshot}
          usage={props.usageByConnection[connecting.id]}
        />
      ) : null}

      {functional || (provider.service_tags ?? []).some((tag) => tag !== "model") ? (
        <div className="model-services-note">
          <WandSparkles aria-hidden="true" className="size-4 shrink-0" />
          <span>{t(functional ? "settings.modelServices.functional.only" : "settings.modelServices.functional.also")}</span>
          <Button onClick={props.onOpenFunctions} size="sm" type="button" variant="link">
            {t("settings.modelServices.functional.open")}
          </Button>
        </div>
      ) : null}

      <section className="model-services-section">
        <div className="model-services-section-head">
          <h3>{t("settings.modelServices.section.accounts")}</h3>
          <small>{accountCount}</small>
        </div>
        <ProviderAccountsSection
          mutations={mutations}
          onSnapshot={props.onSnapshot}
          onUsage={props.onUsage}
          provider={provider}
          snapshot={snapshot}
          usageByConnection={props.usageByConnection}
        />
      </section>

      {!functional ? (
        <section className="model-services-section">
          <div className="model-services-section-head">
            <h3>{t("settings.modelServices.section.models")}</h3>
            {accounts.length > 0 ? (
              <div aria-label={t("settings.modelServices.scope.label")} className="model-services-scope-chips" role="group">
                <button aria-pressed={scope === null} className="model-services-tag-chip" onClick={() => setScopeId(null)} type="button">
                  {t("settings.modelServices.scope.all")}
                </button>
                {accounts.map((connection) => (
                  <button
                    aria-pressed={scope?.id === connection.id}
                    className="model-services-tag-chip"
                    key={connection.id}
                    onClick={() => setScopeId(connection.id)}
                    title={t("settings.modelServices.scope.hint")}
                    type="button"
                  >
                    {connection.display_name}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
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
    </div>
  )
}

// ConnectStepState is the visual state of one connect step.
// ConnectStepState 是一个接入步骤的视觉状态。
type ConnectStepState = "done" | "running" | "warning" | "failed" | "skipped"

// ConnectProgressProps carries the just-added account and the actions its steps offer.
// ConnectProgressProps 承载刚添加的账号及其步骤提供的操作。
interface ConnectProgressProps {
  snapshot: PublicAiSettingsSnapshot
  connection: PublicAiConnection
  usage: PublicAiAccountUsage | undefined
  signupUrl: string | undefined
  mutations: SnapshotMutations
  onUsage: (connectionId: string, usage: PublicAiAccountUsage) => void
  onAdjust: () => void
  onDismiss: () => void
}

// ConnectProgress shows what happened after an account was saved: discovery, default access, usage.
// It reads the saved state only; the one automatic action is a usage read for providers that support it.
// ConnectProgress 显示账号保存之后发生的事情：模型发现、默认访问与用量。
// 它只读取已保存状态；唯一的自动动作是为支持的供应商读取一次用量。
function ConnectProgress(props: ConnectProgressProps) {
  const { t } = useI18n()
  const { snapshot, connection, mutations } = props
  const usageSupported = connection.usage_capability !== "unsupported"
    && (!connection.usage_requires_management_key || connection.has_management_key)
  // usageRequestedRef keeps the automatic usage read to exactly one attempt per account.
  // usageRequestedRef 保证每个账号的自动用量读取只尝试一次。
  const usageRequestedRef = React.useRef<string | null>(null)
  React.useEffect(() => {
    if (!usageSupported || usageRequestedRef.current === connection.id || mutations.pendingKey !== null) return
    usageRequestedRef.current = connection.id
    void mutations.run(`usage/${connection.id}`, async () => {
      props.onUsage(connection.id, await queryPublicAiConnectionUsage(connection.id))
    })
  }, [connection.id, mutations, props, usageSupported])

  const status = snapshot.catalogStatuses[connection.id]
  const catalogError = snapshot.catalogErrors[connection.id]
  const modelCount = snapshot.modelSupportCatalogs[connection.id]?.models.length ?? status?.model_count ?? 0
  const enabledCount = snapshot.catalogs[connection.id]?.models.length ?? 0
  const discovery: ConnectStepState = catalogError ? "failed"
    : status?.state === "ready" ? "done"
      : status?.state === "not_checked" || status === undefined ? "running"
        : "warning"
  const usage = props.usage ?? snapshot.usageCache[connection.id]
  const usageState: ConnectStepState = !usageSupported ? "skipped"
    : mutations.pendingKey === `usage/${connection.id}` ? "running"
      : usage ? "done" : "warning"

  const steps: { key: string; state: ConnectStepState; title: string; detail?: React.ReactNode; action?: React.ReactNode }[] = [
    {
      key: "saved",
      state: connection.pending_validation ? "warning" : "done",
      title: t("settings.modelServices.connect.saved"),
      detail: connection.pending_validation ? t("settings.modelServices.connect.pendingValidation") : undefined,
    },
    {
      key: "discovery",
      state: discovery,
      title: discovery === "done"
        ? t("settings.modelServices.connect.discovered").replace("{count}", String(modelCount))
        : t("settings.modelServices.connect.discovering"),
      detail: catalogError ?? status?.message,
      action: discovery === "done" ? undefined : (
        <Button
          disabled={mutations.pendingKey !== null}
          onClick={() => void mutations.run(`account/${connection.id}`, () => syncPublicAiConnection(connection.id))}
          size="sm"
          type="button"
          variant="outline"
        >{t("settings.modelServices.connect.retry")}</Button>
      ),
    },
    {
      key: "enabled",
      state: discovery === "done" ? "done" : "skipped",
      title: t("settings.modelServices.connect.enabled").replace("{count}", String(enabledCount)),
      action: discovery === "done" ? (
        <Button onClick={props.onAdjust} size="sm" type="button" variant="outline">{t("settings.modelServices.connect.adjust")}</Button>
      ) : undefined,
    },
    {
      key: "usage",
      state: usageState,
      title: usageSupported ? t("settings.modelServices.connect.usage")
        : connection.usage_capability !== "unsupported" ? t("settings.modelServices.connect.usageNeedsKey")
          : t("settings.modelServices.connect.usageUnsupported"),
      action: connection.usage_capability === "unsupported" && props.signupUrl ? (
        <a className="model-services-official-link" href={props.signupUrl} rel="noreferrer" target="_blank">
          <ExternalLink aria-hidden="true" className="size-3.5" />
          {t("settings.modelServices.usage.officialSite")}
        </a>
      ) : undefined,
    },
  ]

  return (
    <section aria-live="polite" className="model-services-connect">
      <div className="model-services-connect-head">
        <strong>{t("settings.modelServices.connect.title").replace("{account}", connection.display_name)}</strong>
        <Button aria-label={t("settings.modelServices.connect.dismiss")} onClick={props.onDismiss} size="icon-xs" type="button" variant="ghost"><X /></Button>
      </div>
      <ol>
        {steps.map((step) => (
          <li className={cn("model-services-connect-step")} data-state={step.state} key={step.key}>
            <ConnectStepIcon state={step.state} />
            <span className="model-services-connect-copy">
              <span>{step.title}</span>
              {step.detail ? <small>{step.detail}</small> : null}
            </span>
            {step.action ?? null}
          </li>
        ))}
      </ol>
    </section>
  )
}

// ConnectStepIcon renders the state glyph of one connect step.
// ConnectStepIcon 渲染一个接入步骤的状态图标。
function ConnectStepIcon(props: { state: ConnectStepState }) {
  switch (props.state) {
    case "done": return <CheckCircle2 aria-hidden="true" className="size-4" />
    case "running": return <LoaderCircle aria-hidden="true" className="size-4 animate-spin" />
    case "warning":
    case "failed": return <CircleAlert aria-hidden="true" className="size-4" />
    case "skipped": return <CircleDashed aria-hidden="true" className="size-4" />
  }
}
