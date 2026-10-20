import * as React from "react"
import { Boxes, CircleCheck, Rows3 } from "lucide-react"

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
import { PageEmptyState, PageTabs } from "@/components/page"
import { Button } from "@/ui-kit/primitives/button"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import { useI18n } from "@/i18n"
import { errorMessage } from "@/lib/display-format"
import { loadPublicAiSettings, savePublicAiProviderPriority } from "@/service/settings-actions-service"
import type {
  PublicAiAccountUsage,
  PublicAiProviderDescriptor,
  PublicAiProviderToolCapabilityKind,
  PublicAiSettingsSnapshot,
} from "@/types"

import { ModelServicesDiagnostics } from "./model-services-diagnostics"
import { alertSeverity, collectProviderAlerts, providerHealth, type ProviderAlert } from "./provider-alerts"
import {
  buildProviderDirectory,
  priorityWritesForOrder,
  providerIsConfigured,
  type ProviderTagFilter,
} from "./provider-directory"
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
  // modelFilter narrows the model-led view to the chosen providers; empty means all.
  // modelFilter 将以模型为主的视图收窄到所选供应商；为空表示全部。
  const [modelFilter, setModelFilter] = React.useState<ReadonlySet<string>>(() => new Set())
  // Dialog state: add account, custom provider editor, model directory, provider removal.
  // 弹窗状态：添加账号、自定义供应商编辑器、模型目录、供应商移除。
  const [addAccount, setAddAccount] = React.useState<{ providerId: string; entryId?: string } | null>(null)
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
  const health = React.useMemo(() => snapshot ? providerHealth(snapshot, alerts) : {}, [snapshot, alerts])
  const accountCounts = React.useMemo(() => {
    const counts: Record<string, number> = {}
    for (const connection of snapshot?.connections ?? []) {
      counts[connection.provider_id] = (counts[connection.provider_id] ?? 0) + 1
    }
    return counts
  }, [snapshot])
  // modelCounts counts each provider's models enabled in conversations.
  // modelCounts 统计每个供应商在会话中启用的模型数量。
  const modelCounts = React.useMemo(() => {
    const counts: Record<string, number> = {}
    for (const record of snapshot?.modelAccessRecords ?? []) {
      if (record.enabled) counts[record.providerId] = (counts[record.providerId] ?? 0) + 1
    }
    return counts
  }, [snapshot])
  // The selection falls back to the pinned core account, the first configured provider, then the first recommendation.
  // 选择依次回退到置顶核心账号、第一个已配置供应商、第一个推荐供应商。
  const selected = snapshot?.providers.find((provider) => provider.id === selectedId)
    ?? directory?.pinned[0]
    ?? directory?.configured[0]
    ?? directory?.available[0]
    ?? null
  // modelProviderIds lists configured model providers, pinned core accounts first, for the model-led view.
  // modelProviderIds 列出已配置的模型供应商（置顶核心账号在前），用于以模型为主的视图。
  const modelProviderIds = React.useMemo(
    () => (snapshot && directory
      ? [...directory.pinned.filter((provider) => providerIsConfigured(snapshot, provider)), ...directory.configured]
        .filter((provider) => !isFunctionalProvider(provider))
        .map((provider) => provider.id)
      : []),
    [directory, snapshot],
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
    setAddAccount(null)
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

  // showProvider leads with the provider view on that provider; rail rows and alerts use it.
  // showProvider 切换到以供应商为主的视图并选中该供应商；目录行与提醒使用它。
  const showProvider = (providerId: string) => {
    setView("provider")
    setSelectedId(providerId)
  }
  const providerName = (providerId: string) => {
    const provider = snapshot?.providers.find((candidate) => candidate.id === providerId)
    return provider ? localizedProviderName(provider.id, provider.display_name, t) : providerId
  }

  const tabs: { value: ModelServicesTab; label: string; count?: number }[] = [
    { value: "providers", label: t("settings.modelServices.tabs.providers") },
    { value: "functions", label: t("settings.modelServices.tabs.functions") },
    { value: "diagnostics", label: t("settings.modelServices.tabs.diagnostics"), count: snapshot?.customProviderManifestFailureTotalCount || undefined },
  ]
  const viewSwitch = (
    <div className="vk-ms-view-switch" role="group" aria-label={t("settings.modelServices.view.label")}>
      <button aria-pressed={view === "provider"} onClick={() => setView("provider")} type="button">
        <Boxes aria-hidden="true" />{t("settings.modelServices.view.provider")}
      </button>
      <button aria-pressed={view === "model"} onClick={() => setView("model")} type="button">
        <Rows3 aria-hidden="true" />{t("settings.modelServices.view.model")}
      </button>
    </div>
  )

  return (
    <div className="vk-ms-page">
      <PageTabs ariaLabel={t("settings.modelProviders")} onValueChange={selectTab} options={tabs} value={tab} />

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
        <div className="vk-ms-shell">
          {snapshot ? <AlertStrip alerts={alerts} onSelect={showProvider} providerName={providerName} /> : null}
          <div className="vk-ms-body">
            <ProviderRail
              accountCounts={accountCounts}
              directory={directory}
              health={health}
              modelCounts={modelCounts}
              onAddCustom={() => setCustomEditor({ provider: null })}
              onOpenDiagnostics={() => selectTab("diagnostics")}
              onQuery={setQuery}
              onReorder={reorder}
              onSelect={showProvider}
              onTag={setTag}
              query={query}
              reorderDisabled={mutations.pendingKey !== null}
              selectedId={view === "provider" ? selected?.id ?? null : null}
              tag={tag}
            />
            <div className="vk-ms-main">
              {!snapshot ? (
                <StreamPlaceholder decorative shape="rows" count={6} />
              ) : view === "model" ? (
                <ModelLedView
                  filter={modelFilter}
                  mutations={mutations}
                  onFilter={setModelFilter}
                  onSnapshot={setSnapshot}
                  providerIds={modelProviderIds}
                  providerName={providerName}
                  snapshot={snapshot}
                  viewSwitch={viewSwitch}
                />
              ) : selected ? (
                <ProviderWorkspace
                  alerts={alerts.filter((alert) => alert.providerId === selected.id)}
                  connectingId={connectingId}
                  mutations={mutations}
                  onAddAccount={(providerId, entryId) => setAddAccount({ providerId, entryId })}
                  onDismissConnecting={() => setConnectingId(null)}
                  onEditCustom={(provider) => setCustomEditor({ provider })}
                  onManageDirectory={setDirectoryProvider}
                  onOpenDiagnostics={() => selectTab("diagnostics")}
                  onOpenFunctions={() => selectTab("functions")}
                  onRemoveCustom={setRemovingProvider}
                  onSnapshot={setSnapshot}
                  onUsage={onUsage}
                  provider={selected}
                  snapshot={snapshot}
                  usageByConnection={usageByConnection}
                  viewSwitch={viewSwitch}
                />
              ) : (
                <PageEmptyState icon={<Boxes />} title={t("settings.modelExperience.noProviders")} />
              )}
            </div>
          </div>
        </div>
      ) : null}

      {tab === "functions" ? <AiCapabilitiesSettingsArea initialCapability={props.initialCapability} /> : null}
      {tab === "diagnostics" ? <ModelServicesDiagnostics mutations={mutations} snapshot={snapshot} /> : null}

      <PublicAiConnectionDialog
        connection={null}
        initialEntryId={addAccount?.entryId}
        initialProviderId={addAccount?.providerId ?? ""}
        lockedProviderId={addAccount?.providerId ?? ""}
        onOpenChange={(open) => {
          if (!open) setAddAccount(null)
        }}
        onSaved={onAccountSaved}
        open={addAccount !== null}
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

// AlertStrip lists only the accounts that need attention; with none it says every account is healthy.
// AlertStrip 只列出需要关注的账号；没有时显示所有账号正常。
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
    <section aria-label={t("settings.modelServices.alerts.title")} className="vk-ms-alerts">
      {props.alerts.length === 0 ? (
        <span className="vk-ms-alert" data-severity="ok">
          <CircleCheck aria-hidden="true" />{t("settings.modelServices.alerts.allOk")}
        </span>
      ) : (
        <>
          <ul>
            {props.alerts.map((alert) => (
              <li key={`${alert.connectionId}:${alert.kind}`}>
                <button
                  className="vk-ms-alert"
                  data-severity={alertSeverity(alert.kind)}
                  onClick={() => props.onSelect(alert.providerId)}
                  title={alert.detail}
                  type="button"
                >
                  <i aria-hidden="true" className="vk-ms-dot" data-state={alertSeverity(alert.kind)} />
                  <strong>{props.providerName(alert.providerId)}</strong>
                  <span>{alert.connectionName}</span>
                  <span>{label(alert)}</span>
                </button>
              </li>
            ))}
          </ul>
          <small>{t("settings.modelServices.alerts.note")}</small>
        </>
      )}
    </section>
  )
}

// ModelLedView lists every configured provider's models, optionally narrowed to some providers.
// Same-named models from different providers stay separate rows.
// ModelLedView 列出所有已配置供应商的模型，可收窄到部分供应商；不同供应商的同名模型保持为独立行。
function ModelLedView(props: {
  snapshot: PublicAiSettingsSnapshot
  providerIds: string[]
  filter: ReadonlySet<string>
  onFilter: (next: ReadonlySet<string>) => void
  providerName: (providerId: string) => string
  mutations: ReturnType<typeof useSnapshotMutations>
  onSnapshot: (next: PublicAiSettingsSnapshot) => void
  viewSwitch: React.ReactNode
}) {
  const { t } = useI18n()
  const shown = props.filter.size > 0 ? props.providerIds.filter((id) => props.filter.has(id)) : props.providerIds
  const toggle = (providerId: string) => {
    const next = new Set(props.filter)
    if (next.has(providerId)) next.delete(providerId)
    else next.add(providerId)
    props.onFilter(next)
  }
  return (
    <div className="vk-ms-workspace">
      <header className="vk-ms-head">
        <div className="vk-ms-head-copy">
          <div className="vk-ms-head-title"><h2>{t("settings.modelServices.view.allModels")}</h2></div>
          <p>{t("settings.modelServices.view.allModelsHint")}</p>
        </div>
        <div className="vk-ms-head-actions">{props.viewSwitch}</div>
      </header>
      {props.providerIds.length === 0 ? (
        <PageEmptyState
          description={t("settings.modelServices.view.emptyDescription")}
          icon={<Rows3 />}
          title={t("settings.modelServices.view.empty")}
        />
      ) : (
        <>
          <div aria-label={t("settings.modelServices.view.providerFilter")} className="vk-ms-scope" role="group">
            <span>{t("settings.modelServices.view.providerFilter")}</span>
            {props.providerIds.map((providerId) => (
              <button
                aria-pressed={props.filter.has(providerId)}
                className="vk-ms-tag-chip"
                key={providerId}
                onClick={() => toggle(providerId)}
                type="button"
              >
                {props.providerName(providerId)}
              </button>
            ))}
          </div>
          <ProviderModelCatalog
            accountScope={null}
            mutations={props.mutations}
            onSnapshot={props.onSnapshot}
            providerIds={shown}
            showProvider
            snapshot={props.snapshot}
          />
        </>
      )}
    </div>
  )
}
