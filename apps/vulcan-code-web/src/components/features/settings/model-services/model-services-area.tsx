import * as React from "react"
import { Boxes, CircleAlert, Rows3, Stethoscope, WandSparkles } from "lucide-react"

import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context"
import { AiCapabilitiesSettingsArea } from "@/components/features/settings/ai-capabilities-settings"
import { CustomProviderModelsDialog } from "@/components/features/settings/custom-provider-models-dialog"
import { CustomProviderDialog } from "@/components/features/settings/provider-management-settings/custom-provider-dialog"
import { PublicAiConnectionDialog } from "@/components/features/settings/provider-list-settings"
import { ProviderModelCatalog } from "@/components/features/settings/provider-model-settings"
import { isFunctionalProvider } from "@/components/features/settings/provider-ordering"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice"
import { SystemProviderModelsDialog } from "@/components/features/settings/system-provider-models-dialog"
import { PageEmptyState } from "@/components/page"
import { Button } from "@/components/ui/button"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { useI18n } from "@/i18n"
import { errorMessage } from "@/lib/display-format"
import { cn } from "@/lib/utils"
import { loadPublicAiSettings, savePublicAiProviderPriority } from "@/service/settings-actions-service"
import type {
  PublicAiAccountUsage,
  PublicAiProviderDescriptor,
  PublicAiProviderToolCapabilityKind,
  PublicAiSettingsSnapshot,
} from "@/types"

import { ModelServicesDiagnostics } from "./model-services-diagnostics"
import { collectProviderAlerts, type ProviderAlert } from "./provider-alerts"
import { buildProviderDirectory, priorityWritesForOrder, type ProviderTagFilter } from "./provider-directory"
import { ProviderRail } from "./provider-rail"
import { ProviderRemovalDialog } from "./provider-removal-dialog"
import { ProviderWorkspace } from "./provider-workspace"
import { useSnapshotMutations } from "./use-snapshot-mutations"

// ModelServicesTab is one top-level section of the model services page.
// ModelServicesTab 是模型服务页的一个顶层分区。
export type ModelServicesTab = "providers" | "functions" | "diagnostics"

// ModelServicesView chooses whether providers or models lead the providers tab.
// ModelServicesView 选择供应商标签页以供应商还是以模型为主。
type ModelServicesView = "provider" | "model"

// ModelServicesAreaProps selects where a deep link lands.
// ModelServicesAreaProps 选择深链接的落点。
export interface ModelServicesAreaProps {
  initialTab?: ModelServicesTab
  initialView?: ModelServicesView
  // initialCapability opens the functions tab on the tab that owns this capability.
  // initialCapability 在功能分配标签页中打开该能力所属的标签。
  initialCapability?: PublicAiProviderToolCapabilityKind | null
}

// ModelServicesArea merges providers, accounts, models and function routing into one page.
// The page owns the one settings snapshot; every write goes through a single serialized lane.
// ModelServicesArea 将供应商、账号、模型与功能分配合并为一个页面。
// 页面持有唯一设置快照；所有写入经由同一条串行通道。
export function ModelServicesArea(props: ModelServicesAreaProps) {
  const { t } = useI18n()
  const account = React.useContext(OpenVulcanAccountContext)
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [loadError, setLoadError] = React.useState<string | null>(null)
  const [tab, setTab] = React.useState<ModelServicesTab>(props.initialTab ?? "providers")
  const [view, setView] = React.useState<ModelServicesView>(props.initialView ?? "provider")
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [tag, setTag] = React.useState<ProviderTagFilter>("all")
  const [query, setQuery] = React.useState("")
  const [usageByConnection, setUsageByConnection] = React.useState<Record<string, PublicAiAccountUsage>>({})
  // Dialog state: add account, custom provider editor, model directory, provider removal.
  // 弹窗状态：添加账号、自定义供应商编辑器、模型目录、供应商移除。
  const [addAccountProviderId, setAddAccountProviderId] = React.useState("")
  const [customEditor, setCustomEditor] = React.useState<{ provider: PublicAiProviderDescriptor | null } | null>(null)
  const [directoryProvider, setDirectoryProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [removingProvider, setRemovingProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  // connectingId is the account just added; its connect progress stays until dismissed.
  // connectingId 是刚添加的账号；其接入进度保留到用户关闭。
  const [connectingId, setConnectingId] = React.useState<string | null>(null)
  const mutations = useSnapshotMutations(setSnapshot)

  const reload = React.useCallback(async () => {
    setLoadError(null)
    try {
      setSnapshot(await loadPublicAiSettings())
    } catch (reason) {
      setLoadError(errorMessage(reason))
    }
  }, [])
  // Account changes alter the visible core-provider directory.
  // 账号变化会改变可见的核心供应商目录。
  React.useEffect(() => {
    void reload()
  }, [reload, account?.snapshot?.connectionId, account?.snapshot?.connected])

  // selectTab reloads when leaving the functions tab, whose editors save through their own snapshot.
  // selectTab 在离开功能分配标签页时重新读取，因为其编辑器通过自身快照保存。
  const selectTab = (next: ModelServicesTab) => {
    if (tab === "functions" && next !== "functions") void reload()
    setTab(next)
  }

  const directory = React.useMemo(() => snapshot ? buildProviderDirectory(snapshot) : null, [snapshot])
  const alerts = React.useMemo(
    () => snapshot ? collectProviderAlerts(snapshot, usageByConnection) : [],
    [snapshot, usageByConnection],
  )
  const attention = React.useMemo(() => new Set(alerts.map((alert) => alert.providerId)), [alerts])
  const accountCounts = React.useMemo(() => {
    const counts: Record<string, number> = {}
    for (const connection of snapshot?.connections ?? []) {
      counts[connection.provider_id] = (counts[connection.provider_id] ?? 0) + 1
    }
    return counts
  }, [snapshot])
  // The selection falls back to the first configured provider, then the first recommendation.
  // 选择回退到第一个已配置供应商，其次是第一个推荐供应商。
  const selected = snapshot?.providers.find((provider) => provider.id === selectedId)
    ?? directory?.configured[0]
    ?? directory?.available[0]
    ?? null
  // modelProviderIds lists configured model providers in the user's order for the model-led view.
  // modelProviderIds 按用户顺序列出已配置的模型供应商，用于以模型为主的视图。
  const modelProviderIds = React.useMemo(
    () => directory?.configured.filter((provider) => !isFunctionalProvider(provider)).map((provider) => provider.id) ?? [],
    [directory],
  )

  const onUsage = React.useCallback((connectionId: string, usage: PublicAiAccountUsage) => {
    setUsageByConnection((current) => ({ ...current, [connectionId]: usage }))
  }, [])

  // reorder saves the minimal priority writes in sequence; a partial failure reloads the truth.
  // reorder 依次保存最少的优先级写入；部分失败时重新读取真实状态。
  const reorder = (order: string[]) => {
    if (!snapshot) return
    const writes = priorityWritesForOrder(order, snapshot.providerPriorities)
    if (writes.length === 0) return
    void mutations.run("provider-order", async () => {
      let latest: PublicAiSettingsSnapshot | undefined
      for (const write of writes) {
        latest = await savePublicAiProviderPriority(write.providerId, write.priority)
      }
      return latest
    }, { reconcile: true })
  }

  // onAccountSaved applies the saved snapshot and starts connect progress for a newly added account.
  // onAccountSaved 应用保存后的快照，并为新添加的账号开始接入进度。
  const onAccountSaved = (next: PublicAiSettingsSnapshot) => {
    const known = new Set(snapshot?.connections.map((connection) => connection.id) ?? [])
    const added = next.connections.find((connection) => !known.has(connection.id))
    setSnapshot(next)
    setAddAccountProviderId("")
    if (added) {
      setSelectedId(added.provider_id)
      setConnectingId(added.id)
    }
  }

  // onCustomSaved selects a newly created custom provider and continues into its model directory.
  // onCustomSaved 选中新建的自定义供应商，并继续进入其模型目录。
  const onCustomSaved = (next: PublicAiSettingsSnapshot) => {
    const known = new Set(snapshot?.providers.map((provider) => provider.id) ?? [])
    const created = next.providers.filter((provider) => provider.kind === "custom" && !known.has(provider.id))
    setSnapshot(next)
    if (created.length === 1) {
      setSelectedId(created[0].id)
      setDirectoryProvider(created[0])
    }
  }

  const tabs: { value: ModelServicesTab; label: string; icon: React.ReactNode; count?: number }[] = [
    { value: "providers", label: t("settings.modelServices.tabs.providers"), icon: <Boxes aria-hidden="true" /> },
    { value: "functions", label: t("settings.modelServices.tabs.functions"), icon: <WandSparkles aria-hidden="true" /> },
    { value: "diagnostics", label: t("settings.modelServices.tabs.diagnostics"), icon: <Stethoscope aria-hidden="true" />, count: snapshot?.customProviderManifestFailureTotalCount || undefined },
  ]

  return (
    <div className="model-services-page">
      <div className="model-services-tabs" role="tablist" aria-label={t("settings.modelProviders")}>
        {tabs.map((option) => (
          <button
            aria-selected={tab === option.value}
            className="model-services-tab"
            key={option.value}
            onClick={() => selectTab(option.value)}
            role="tab"
            type="button"
          >
            {option.icon}
            <span>{option.label}</span>
            {option.count ? <small>{option.count}</small> : null}
          </button>
        ))}
      </div>

      {mutations.error ? (
        <SettingsErrorNotice
          action={<Button onClick={mutations.clearError} size="sm" type="button" variant="ghost">{t("settings.modelServices.dismiss")}</Button>}
          detail={mutations.error}
          message={t("settings.modelServices.saveFailed")}
        />
      ) : null}
      {loadError ? (
        <SettingsErrorNotice
          action={<Button onClick={() => void reload()} size="sm" type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button>}
          detail={loadError}
          message={t("settings.modelServices.loadFailed")}
        />
      ) : null}

      {tab === "providers" ? (
        <>
          {alerts.length > 0 ? (
            <AlertStrip
              alerts={alerts}
              onSelect={(providerId) => {
                setView("provider")
                setSelectedId(providerId)
              }}
              providerName={(providerId) => {
                const provider = snapshot?.providers.find((candidate) => candidate.id === providerId)
                return provider ? localizedProviderName(provider.id, provider.display_name, t) : providerId
              }}
            />
          ) : null}
          <div className="model-services-view-switch" role="group" aria-label={t("settings.modelServices.view.label")}>
            <button aria-pressed={view === "provider"} onClick={() => setView("provider")} type="button">
              <Boxes aria-hidden="true" />{t("settings.modelServices.view.provider")}
            </button>
            <button aria-pressed={view === "model"} onClick={() => setView("model")} type="button">
              <Rows3 aria-hidden="true" />{t("settings.modelServices.view.model")}
            </button>
          </div>
          {view === "provider" ? (
            <div className="model-services-split">
              <ProviderRail
                accountCounts={accountCounts}
                attention={attention}
                directory={directory}
                onAddCustom={() => setCustomEditor({ provider: null })}
                onQuery={setQuery}
                onReorder={reorder}
                onSelect={setSelectedId}
                onTag={setTag}
                query={query}
                reorderDisabled={mutations.pendingKey !== null}
                selectedId={selected?.id ?? null}
                tag={tag}
              />
              <div className="model-services-main">
                {!snapshot ? (
                  <StreamPlaceholder decorative shape="rows" count={6} />
                ) : selected ? (
                  <ProviderWorkspace
                    connectingId={connectingId}
                    mutations={mutations}
                    onAddAccount={setAddAccountProviderId}
                    onDismissConnecting={() => setConnectingId(null)}
                    onEditCustom={(provider) => setCustomEditor({ provider })}
                    onManageDirectory={setDirectoryProvider}
                    onOpenFunctions={() => selectTab("functions")}
                    onRemoveCustom={setRemovingProvider}
                    onSnapshot={setSnapshot}
                    onUsage={onUsage}
                    provider={selected}
                    snapshot={snapshot}
                    usageByConnection={usageByConnection}
                  />
                ) : (
                  <PageEmptyState icon={<Boxes />} title={t("settings.modelExperience.noProviders")} />
                )}
              </div>
            </div>
          ) : !snapshot ? (
            <StreamPlaceholder decorative shape="rows" count={8} />
          ) : modelProviderIds.length === 0 ? (
            <PageEmptyState
              description={t("settings.modelServices.view.emptyDescription")}
              icon={<Rows3 />}
              title={t("settings.modelServices.view.empty")}
            />
          ) : (
            <ProviderModelCatalog
              accountScope={null}
              mutations={mutations}
              onSnapshot={setSnapshot}
              providerIds={modelProviderIds}
              showProvider
              snapshot={snapshot}
            />
          )}
        </>
      ) : null}

      {tab === "functions" ? <AiCapabilitiesSettingsArea initialCapability={props.initialCapability} /> : null}
      {tab === "diagnostics" ? <ModelServicesDiagnostics mutations={mutations} snapshot={snapshot} /> : null}

      <PublicAiConnectionDialog
        connection={null}
        initialProviderId={addAccountProviderId}
        lockedProviderId={addAccountProviderId}
        onOpenChange={(open) => {
          if (!open) setAddAccountProviderId("")
        }}
        onSaved={onAccountSaved}
        open={addAccountProviderId !== ""}
        providers={snapshot?.providers ?? []}
      />
      <CustomProviderDialog
        connections={snapshot?.connections ?? []}
        modelOverrideRecords={snapshot?.modelOverrideRecords ?? []}
        onOpenChange={(open) => {
          if (!open) setCustomEditor(null)
        }}
        onSaved={onCustomSaved}
        open={customEditor !== null}
        provider={customEditor?.provider ?? null}
      />
      {directoryProvider?.kind === "custom" ? (
        <CustomProviderModelsDialog
          onOpenChange={(open) => {
            if (!open) setDirectoryProvider(null)
          }}
          onSaved={setSnapshot}
          open
          provider={directoryProvider}
          snapshot={snapshot}
        />
      ) : null}
      {directoryProvider?.kind === "system" && snapshot ? (
        <SystemProviderModelsDialog
          onOpenChange={(open) => {
            if (!open) setDirectoryProvider(null)
          }}
          onSaved={setSnapshot}
          open
          provider={directoryProvider}
          snapshot={snapshot}
        />
      ) : null}
      <ProviderRemovalDialog
        mutations={mutations}
        onClose={() => setRemovingProvider(null)}
        onRemoved={() => {
          setSelectedId(null)
          setRemovingProvider(null)
        }}
        provider={removingProvider}
        snapshot={snapshot}
      />
    </div>
  )
}

// AlertStrip lists only the accounts that need attention; healthy accounts never appear here.
// AlertStrip 只列出需要关注的账号；正常账号不会出现在这里。
function AlertStrip(props: {
  alerts: ProviderAlert[]
  providerName: (providerId: string) => string
  onSelect: (providerId: string) => void
}) {
  const { t } = useI18n()
  const label = (alert: ProviderAlert) => {
    switch (alert.kind) {
      case "catalog_error": return t("settings.modelServices.alerts.catalogError")
      case "pending_validation": return t("settings.modelServices.alerts.pendingValidation")
      case "quota_low": return t("settings.modelServices.alerts.quotaLow")
    }
  }
  return (
    <section aria-label={t("settings.modelServices.alerts.title")} className="model-services-alerts">
      <CircleAlert aria-hidden="true" className="size-4 shrink-0" />
      <ul>
        {props.alerts.map((alert) => (
          <li key={`${alert.connectionId}:${alert.kind}`}>
            <button
              className={cn("model-services-alert")}
              data-kind={alert.kind}
              onClick={() => props.onSelect(alert.providerId)}
              title={alert.detail}
              type="button"
            >
              <strong>{props.providerName(alert.providerId)}</strong>
              <span>{alert.connectionName}</span>
              <span>{label(alert)}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
