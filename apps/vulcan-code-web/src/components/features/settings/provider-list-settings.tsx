import * as React from "react"
import { Boxes, ExternalLink, KeyRound, Layers3, ListChecks, Plus, RefreshCw, Star, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { CredentialModelAccessDialog } from "@/components/features/settings/credential-model-access-dialog"
import { localizedServiceEntryPresentation } from "@/components/features/settings/provider-presentation"
import {
  CredentialUsageOverview,
  usageCapabilityRequiresManagementKey,
} from "@/components/public-ai/credential-usage-overview"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { AdaptiveToolPanel } from "@/components/workspace/adaptive-tool-panel"
import { WorkspaceToolPanelMenu } from "@/components/workspace/workspace-tool-panel-menu"
import { useI18n } from "@/i18n"
import {
  ExternalUrlConfirmationRequiredError,
  openExternalUrl,
} from "@/service/project-actions-service"
import {
  addPublicAiConnection,
  cancelPublicAiOAuth,
  commitPublicAiOAuthConnection,
  getPublicAiOAuthStatus,
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
  removePublicAiConnection,
  savePublicAiDefaultConnection,
  savePublicAiUsageRefreshSettings,
  startPublicAiOAuth,
  syncPublicAiConnection,
  updatePublicAiConnection,
} from "@/service/settings-actions-service"
import type {
  CredentialMode,
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiCredentialProfile,
  PublicAiOAuthTransaction,
  PublicAiProviderDescriptor,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

// CredentialManagementArea renders credential-only account management and runtime metadata.
// CredentialManagementArea 渲染仅凭据账号管理与运行时元数据。
export function CredentialManagementArea(props: {
  // Controls the header-owned add-connection dialog.
  // 控制标题栏持有的新增连接弹窗。
  onProviderPickerOpenChange: (open: boolean) => void
  // Whether the add-connection dialog is open.
  // 新增连接弹窗是否打开。
  pickerOpen: boolean
}) {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [busyConnectionId, setBusyConnectionId] = React.useState<string | null>(null)
  const [editingConnection, setEditingConnection] = React.useState<PublicAiConnection | null>(null)
  const [deletingConnection, setDeletingConnection] = React.useState<PublicAiConnection | null>(null)
  const [modelAccessConnection, setModelAccessConnection] = React.useState<PublicAiConnection | null>(null)
  const [usageByConnection, setUsageByConnection] = React.useState<Record<string, PublicAiAccountUsage>>({})
  const [activeProviderId, setActiveProviderId] = React.useState<string>("")
  const [creatingProviderId, setCreatingProviderId] = React.useState<string>("")
  const [menuExpanded, setMenuExpanded] = React.useState(true)
  const { t } = useI18n()

  // refresh loads the authoritative persisted graph and current process-local catalogs.
  // refresh 加载权威持久化连接图与当前进程内目录。
  const refresh = React.useCallback(async () => {
    try {
      setError(null)
      const next = await loadPublicAiSettings()
      setSnapshot(next)
      setUsageByConnection(next.usageCache)
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh])

  // runConnectionAction serializes visual feedback for one exact connection.
  // runConnectionAction 为一个精确连接串行化视觉反馈。
  const runConnectionAction = React.useCallback(async (
    connectionId: string,
    action: () => Promise<PublicAiSettingsSnapshot>,
  ) => {
    setBusyConnectionId(connectionId)
    setError(null)
    try {
      setSnapshot(await action())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyConnectionId(null)
    }
  }, [])

  // refreshUsage queries and displays one account-scoped usage observation.
  // refreshUsage 查询并展示一个账号作用域用量观测。
  const refreshUsage = React.useCallback(async (connectionId: string) => {
    setBusyConnectionId(connectionId)
    setError(null)
    try {
      const usage = await queryPublicAiConnectionUsage(connectionId)
      setUsageByConnection((current) => ({ ...current, [connectionId]: usage }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyConnectionId(null)
    }
  }, [])

  // directoryProviders keeps configured providers first and zero-credential onboarding entries last.
  // directoryProviders 将已配置供应商排在前面，将零凭据引导入口排在最后。
  const directoryProviders = React.useMemo(() => {
    if (!snapshot) {
      return []
    }
    // configuredProviderIds is the exact provider set backed by real credentials.
    // configuredProviderIds 是由真实凭据支持的精确供应商集合。
    const configuredProviderIds = new Set(
      snapshot.connections.map((connection) => connection.provider_id),
    )
    // configuredProviders retain registry order within the real-credential group.
    // configuredProviders 在真实凭据分组内保持注册表顺序。
    const configuredProviders = snapshot.providers.filter((provider) => (
      configuredProviderIds.has(provider.id)
    ))
    // onboardingProviders retain only pinned entries without a credential.
    // onboardingProviders 仅保留没有凭据的固定引导入口。
    const onboardingProviders = snapshot.providers.filter((provider) => (
      provider.credential_directory_visibility === "pinned_onboarding"
      && !configuredProviderIds.has(provider.id)
    ))
    return [...configuredProviders, ...onboardingProviders]
  }, [snapshot])
  // visibleConnections applies the exact left-menu provider filter.
  // visibleConnections 应用精确的左侧菜单供应商筛选。
  const visibleConnections = snapshot?.connections.filter((connection) => (
    !activeProviderId || connection.provider_id === activeProviderId
  )) ?? []
  // activeProviderLabel supplies the shared collapsed-menu pill label.
  // activeProviderLabel 为共享收起菜单胶囊提供标签。
  const activeProviderLabel = activeProviderId
    ? directoryProviders.find((provider) => provider.id === activeProviderId)?.display_name ?? activeProviderId
    : "全部凭据"
  // selectedOnboardingProvider exists only after the user selects one zero-credential entry.
  // selectedOnboardingProvider 仅在用户选中一个零凭据入口后存在。
  const selectedOnboardingProvider = directoryProviders.find((provider) => (
    provider.credential_directory_visibility === "pinned_onboarding"
    && activeProviderId === provider.id
    && !snapshot?.connections.some((connection) => connection.provider_id === provider.id)
  ))

  // Reset a stale filter after the last credential of a configured-only provider is removed.
  // 在仅已配置供应商的最后一个凭据被移除后重置过期筛选。
  React.useEffect(() => {
    if (activeProviderId && !directoryProviders.some((provider) => provider.id === activeProviderId)) {
      setActiveProviderId("")
    }
  }, [activeProviderId, directoryProviders])

  // openPromotion opens the exact registered signup or referral URL through the desktop host.
  // openPromotion 通过桌面宿主打开已注册的精确注册或推广链接。
  const openPromotion = React.useCallback(async (provider: PublicAiProviderDescriptor) => {
    // promotion is the exact backend-validated public relationship metadata.
    // promotion 是后端已校验的精确公开合作关系元数据。
    const promotion = provider.promotion
    // url prefers a Vulcan-owned referral only when the contract explicitly provides one.
    // url 仅在契约显式提供时优先使用 Vulcan 自有推广链接。
    const url = promotion?.referral_url ?? promotion?.signup_url
    if (!url) {
      return
    }
    try {
      await openExternalUrl(url)
    } catch (reason) {
      if (reason instanceof ExternalUrlConfirmationRequiredError) {
        toast.warning(t("settings.providerOnboarding.externalConfirmation"), {
          description: reason.url,
          duration: 15_000,
          action: {
            label: t("settings.providerOnboarding.externalOpenAnyway"),
            onClick: () => {
              void openExternalUrl(reason.url, true).catch((confirmedReason: unknown) => {
                toast.error(t("settings.providerOnboarding.externalOpenFailed"), {
                  description: errorMessage(confirmedReason),
                })
              })
            },
          },
        })
        return
      }
      setError(errorMessage(reason))
    }
  }, [t])

  // saveUsageRefreshInterval persists the background refresh cadence.
  // saveUsageRefreshInterval 持久化后台刷新频率。
  const saveUsageRefreshInterval = React.useCallback(async (intervalSeconds: number) => {
    try {
      setError(null)
      const next = await savePublicAiUsageRefreshSettings(intervalSeconds)
      setSnapshot(next)
      setUsageByConnection(next.usageCache)
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }, [])

  const modelAccessProvider = snapshot && modelAccessConnection
    ? providerForConnection(snapshot, modelAccessConnection)
    : undefined
  const modelAccessEntry = modelAccessConnection
    ? entryForConnection(modelAccessProvider, modelAccessConnection)
    : undefined

  return (
    <AdaptiveToolPanel
      className="workspace-tool-panel-page workspace-skills-settings credential-management-layout"
      contentClassName="workspace-skills-content credential-management-content"
      contentShellClassName="credential-management-content-shell"
      expanded={menuExpanded}
      minContentWidth={336}
      panel={(
        <WorkspaceToolPanelMenu
          activeLabel={activeProviderLabel}
          ariaLabel="凭据分类"
          className="credential-management-menu"
          collapseLabel="收起为胶囊"
          expanded={menuExpanded}
          expandLabel="展开凭据分类"
          heading="凭据分类"
          onExpandedChange={setMenuExpanded}
        >
          <button
            className={`workspace-tool-panel-menu-row ${activeProviderId === "" ? "active" : ""}`}
            onClick={() => setActiveProviderId("")}
            type="button"
          >
            <Layers3 size={15} />
            <span>全部凭据</span>
            <small>{snapshot?.connections.length ?? 0}</small>
          </button>
          {directoryProviders.map((provider) => (
            <button
              className={`workspace-tool-panel-menu-row ${activeProviderId === provider.id ? "active" : ""}`}
              key={provider.id}
              onClick={() => setActiveProviderId(provider.id)}
              type="button"
            >
              <ProviderIcon category={provider.category} className="size-5" providerId={provider.id} />
              <span>{provider.display_name}</span>
              <small>{snapshot?.connections.filter((connection) => connection.provider_id === provider.id).length ?? 0}</small>
            </button>
          ))}
        </WorkspaceToolPanelMenu>
      )}
      panelWidth={176}
      side="left"
    >
      <div className="settings-direct-content credential-management-surface">
        {snapshot ? (
          <div className="settings-inline-control">
            <span>账号用量后台刷新</span>
            <Select onValueChange={(value) => void saveUsageRefreshInterval(Number(value))} value={String(snapshot.usageRefreshIntervalSeconds)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="60">每 1 分钟</SelectItem>
                <SelectItem value="300">每 5 分钟</SelectItem>
                <SelectItem value="900">每 15 分钟</SelectItem>
                <SelectItem value="1800">每 30 分钟</SelectItem>
                <SelectItem value="3600">每 1 小时</SelectItem>
              </SelectContent>
            </Select>
            <small>仅刷新声明支持用量查询且已具备必要凭据的账号，缓存只保存在内存中。</small>
          </div>
        ) : null}
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        {!snapshot ? (
          <article className="settings-card">
            <strong>正在连接公共 AI 运行时</strong>
            <p>启动后会从各凭据实时获取模型，不读取本地模型缓存。</p>
          </article>
        ) : visibleConnections.length > 0 ? (
          <div className="credential-management-list">
            {visibleConnections.map((connection) => {
              const provider = providerForConnection(snapshot, connection)
              const entry = entryForConnection(provider, connection)
              const credentialProfile = credentialProfileForConnection(entry, connection)
              const usage = usageByConnection[connection.id] ?? snapshot.usageCache[connection.id]
              // Provider-tool entries do not own a model catalog, so historical model-sync errors are irrelevant.
              // 供应商工具入口不拥有模型目录，因此历史模型同步错误与其无关。
              const catalogError = entry?.kind === "provider_tools"
                ? undefined
                : snapshot.catalogErrors[connection.id]
              const isDefault = snapshot.providerDefaults[connection.provider_id] === connection.id
              return (
                <CredentialAccountCard
                  busy={busyConnectionId === connection.id}
                  catalogError={catalogError}
                  connection={connection}
                  credentialProfile={credentialProfile}
                  entry={entry}
                  isDefault={isDefault}
                  key={connection.id}
                  onDelete={() => setDeletingConnection(connection)}
                  onEdit={() => setEditingConnection(connection)}
                  onManageModels={() => setModelAccessConnection(connection)}
                  onRefreshUsage={() => void refreshUsage(connection.id)}
                  onSetDefault={() => void runConnectionAction(
                    connection.id,
                    () => savePublicAiDefaultConnection({
                      providerId: connection.provider_id,
                      connectionId: connection.id,
                    }),
                  )}
                  onSync={() => void runConnectionAction(
                    connection.id,
                    () => syncPublicAiConnection(connection.id),
                  )}
                  provider={provider}
                  usage={usage}
                />
              )
            })}
          </div>
        ) : selectedOnboardingProvider ? (
          <ProviderOnboardingPanel
            onAddCredential={() => {
              setCreatingProviderId(selectedOnboardingProvider.id)
              props.onProviderPickerOpenChange(true)
            }}
            onOpenPromotion={() => void openPromotion(selectedOnboardingProvider)}
            provider={selectedOnboardingProvider}
          />
        ) : activeProviderId ? (
          <div className="flex min-h-48 w-full flex-col items-center justify-center gap-2 text-center text-muted-foreground">
            <strong>{t("settings.providerOnboarding.filteredEmpty")}</strong>
            <p>{t("settings.providerOnboarding.filteredEmptyDescription")}</p>
          </div>
        ) : (
          <div className="flex min-h-48 w-full flex-col items-center justify-center gap-2 text-center text-muted-foreground">
            <strong>尚未添加模型账号</strong>
            <p>点击“添加凭据”，选择供应商、服务入口、凭据方式与协议。</p>
          </div>
        )}
        <PublicAiConnectionDialog
          connection={null}
          initialProviderId={creatingProviderId}
          onOpenChange={(open) => {
            props.onProviderPickerOpenChange(open)
            if (!open) {
              setCreatingProviderId("")
            }
          }}
          onSaved={(next) => {
            setSnapshot(next)
            setCreatingProviderId("")
            props.onProviderPickerOpenChange(false)
          }}
          open={props.pickerOpen}
          providers={snapshot?.providers ?? []}
        />
      <CredentialModelAccessDialog
        connection={modelAccessConnection}
        entry={modelAccessEntry}
        onOpenChange={(open) => {
          if (!open) {
            setModelAccessConnection(null)
          }
        }}
        onSnapshotChange={setSnapshot}
        open={modelAccessConnection !== null}
        provider={modelAccessProvider}
        snapshot={snapshot}
      />
      <PublicAiConnectionDialog
        connection={editingConnection}
        initialProviderId=""
        onOpenChange={(open) => {
          if (!open) {
            setEditingConnection(null)
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setEditingConnection(null)
        }}
        open={editingConnection !== null}
        providers={snapshot?.providers ?? []}
      />
      <PublicAiDeleteConnectionDialog
        connection={deletingConnection}
        connections={snapshot?.connections ?? []}
        isProviderDefault={Boolean(
          deletingConnection
          && snapshot?.providerDefaults[deletingConnection.provider_id] === deletingConnection.id
        )}
        onOpenChange={(open) => {
          if (!open) {
            setDeletingConnection(null)
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setDeletingConnection(null)
        }}
        open={deletingConnection !== null}
        />
      </div>
    </AdaptiveToolPanel>
  )
}

// ProviderOnboardingPanelProps defines one selected zero-credential provider introduction.
// ProviderOnboardingPanelProps 定义一个已选中的零凭据供应商介绍区域。
interface ProviderOnboardingPanelProps {
  // onAddCredential opens the connection dialog preselected to this provider.
  // onAddCredential 打开预选当前供应商的连接弹窗。
  onAddCredential: () => void
  // onOpenPromotion opens the provider-owned signup or referral destination.
  // onOpenPromotion 打开供应商注册或推广目标地址。
  onOpenPromotion: () => void
  // provider is the exact pinned provider descriptor.
  // provider 是精确的固定供应商描述。
  provider: PublicAiProviderDescriptor
}

// ProviderOnboardingPanel renders one full-width introduction without card chrome.
// ProviderOnboardingPanel 在不使用卡片外观的情况下渲染一个满宽介绍区域。
function ProviderOnboardingPanel(props: ProviderOnboardingPanelProps) {
  const { t } = useI18n()
  // promotionLabel distinguishes configured referral relationships from neutral registration.
  // promotionLabel 区分已配置推广关系与中性注册链接。
  const promotionLabel = props.provider.promotion?.partnership_kind === "referral"
    ? t("settings.providerOnboarding.referralSignup")
    : t("settings.providerOnboarding.register")
  // verifiedProtocols lists only protocol routes the backend explicitly marked usable.
  // verifiedProtocols 仅列出后端明确标记为可用的协议路由。
  const verifiedProtocols = Array.from(new Set(
    props.provider.service_entries.flatMap((entry) => (
      entry.protocol_routes
        .filter((route) => route.verified)
        .map((route) => protocolDisplayName(route.protocol_profile_id))
    )),
  ))
  // disclosure selects the localized commercial disclosure from the exact partnership kind.
  // disclosure 根据精确合作类型选择本地化商业披露。
  const disclosure = props.provider.promotion
    ? t(props.provider.promotion.disclosure_key)
    : null

  return (
    <section className="w-full px-1 py-4">
      <div className="flex items-start gap-4 border-b pb-6">
        <ProviderIcon
          category={props.provider.category}
          className="size-12"
          providerId={props.provider.id}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-xl font-semibold">{props.provider.display_name}</h2>
            <Badge variant="secondary">{t("settings.providerOnboarding.notConfigured")}</Badge>
          </div>
          <p className="mt-2 max-w-3xl text-sm text-muted-foreground">
            {t("settings.providerOnboarding.description")}
          </p>
        </div>
      </div>
      <div className="grid w-full gap-6 py-6 md:grid-cols-[minmax(0,1fr)_auto]">
        <div className="space-y-3">
          <p className="text-sm font-medium">
            {props.provider.service_entries.length} {t("settings.providerOnboarding.serviceEntries")}
          </p>
          <div className="flex flex-wrap gap-1.5">
            {props.provider.service_entries.map((entry) => (
              <Badge key={entry.id} variant="secondary">{entry.display_name}</Badge>
            ))}
          {verifiedProtocols.map((protocol) => (
            <Badge key={protocol} variant="outline">{protocol}</Badge>
          ))}
          </div>
          {props.provider.promotion ? (
            <p className="text-sm text-muted-foreground">{disclosure}</p>
          ) : null}
        </div>
        <div className="flex items-start gap-2 md:justify-end">
          {props.provider.promotion ? (
            <Button onClick={props.onOpenPromotion} size="sm" type="button" variant="outline">
              <ExternalLink className="size-3.5" />
              {promotionLabel}
            </Button>
          ) : null}
          <Button onClick={props.onAddCredential} size="sm" type="button">
            <Plus className="size-3.5" />
            {t("settings.providerOnboarding.addCredential")}
          </Button>
        </div>
      </div>
    </section>
  )
}

// CredentialAccountCardProps defines one compact account card backed by exact runtime state.
// CredentialAccountCardProps 定义一个由精确运行时状态支持的紧凑账号卡片。
interface CredentialAccountCardProps {
  // busy prevents duplicate account operations.
  // busy 防止重复账号操作。
  busy: boolean
  // catalogError reports the latest exact catalog failure.
  // catalogError 报告最新精确目录失败。
  catalogError?: string
  // connection identifies the credential account.
  // connection 标识凭据账号。
  connection: PublicAiConnection
  // credentialProfile describes acquisition and usage capabilities.
  // credentialProfile 描述获取方式与用量能力。
  credentialProfile?: PublicAiCredentialProfile
  // entry describes the selected provider endpoint.
  // entry 描述已选供应商入口。
  entry?: PublicAiServiceEntry
  // isDefault identifies the provider's default account.
  // isDefault 标识供应商默认账号。
  isDefault: boolean
  // onDelete opens the destructive confirmation.
  // onDelete 打开破坏性确认。
  onDelete: () => void
  // onEdit opens exact credential editing.
  // onEdit 打开精确凭据编辑。
  onEdit: () => void
  // onRefreshUsage queries current account usage.
  // onRefreshUsage 查询当前账号用量。
  onRefreshUsage: () => void
  // onManageModels opens account-scoped model access management.
  // onManageModels 打开账号作用域模型访问管理。
  onManageModels: () => void
  // onSetDefault selects this account as provider default.
  // onSetDefault 将此账号设为供应商默认账号。
  onSetDefault: () => void
  // onSync refreshes the model catalog.
  // onSync 刷新模型目录。
  onSync: () => void
  // provider is the exact registered owner.
  // provider 是精确注册所有者。
  provider?: PublicAiProviderDescriptor
  // usage is the latest memory-only observation.
  // usage 是最新的仅内存观测。
  usage?: PublicAiAccountUsage
}

// CredentialAccountCard renders the Model Core account-card hierarchy with Vulcan Code actions.
// CredentialAccountCard 使用 Vulcan Code 操作渲染 Model Core 账号卡片层级。
function CredentialAccountCard(props: CredentialAccountCardProps) {
  const { t } = useI18n()
  const usageSupported = props.credentialProfile?.account_usage_query !== "unsupported"
    && (!props.connection.usage_requires_management_key || props.connection.has_management_key)
  const providerName = props.provider?.display_name ?? props.connection.provider_id
  // serviceEntryPresentation separates Alibaba product names from composable scope and region tags.
  // serviceEntryPresentation 将阿里云产品名称与可组合的账号范围、区域标签分离。
  const serviceEntryPresentation = props.entry
    ? localizedServiceEntryPresentation(props.connection.provider_id, props.entry, t)
    : { title: providerName, tags: [] }
  return (
    <Card
      className="h-60 w-full max-w-[21rem] gap-0 border border-border pt-2 ring-0"
      size="sm"
    >
      <CardHeader className="shrink-0">
        <div className="flex items-start justify-between gap-3">
          <ProviderIcon
            category={props.provider?.category}
            className="size-8 shrink-0"
            providerId={props.connection.provider_id}
            serviceEntryId={props.connection.service_entry_id}
          />
          <div className="min-w-0 flex-1">
            <div className="mb-1 truncate text-xs text-muted-foreground">
              {serviceEntryPresentation.title}
            </div>
            <CardTitle className="truncate">{props.connection.display_name}</CardTitle>
            <CardDescription className="mt-0.5 flex min-h-4 flex-wrap items-center gap-1.5">
              {serviceEntryPresentation.tags.map((tag) => (
                <Badge className="h-4 px-1.5 text-[10px]" key={tag} variant="outline">
                  {tag}
                </Badge>
              ))}
              {props.connection.pending_validation ? (
                <Badge className="h-4 px-1.5 text-[10px]" variant="destructive">
                  {t("settings.customModels.pendingValidation")}
                </Badge>
              ) : null}
            </CardDescription>
          </div>
          {usageSupported ? (
            <CredentialIconAction
              disabled={props.busy}
              label="刷新账号用量"
              onClick={props.onRefreshUsage}
            >
              <RefreshCw className={`size-4 ${props.busy ? "animate-spin" : ""}`} />
            </CredentialIconAction>
          ) : null}
        </div>
      </CardHeader>

      <CardContent className="mt-0.5 flex min-h-0 flex-1 flex-col gap-2">
        <CredentialUsageOverview
          connection={props.connection}
          credentialProfile={props.credentialProfile}
          usage={props.usage}
        />
        {props.catalogError ? (
          <p className="text-xs text-destructive" role="status">
            {props.catalogError}
          </p>
        ) : null}
      </CardContent>

      <CardFooter className="mt-auto shrink-0 flex-wrap justify-end gap-2 px-3 py-1.5">
        {props.entry?.kind !== "provider_tools" ? (
          <CredentialIconAction
            disabled={props.busy || props.connection.pending_validation}
            label={t("settings.credentialModels.manage")}
            onClick={props.onManageModels}
          >
            <ListChecks className="size-4" />
          </CredentialIconAction>
        ) : null}
        {props.entry?.kind !== "provider_tools" ? (
          <CredentialIconAction
            disabled={props.busy || props.connection.pending_validation}
            label="同步支持模型"
            onClick={props.onSync}
          >
            <Boxes className="size-4" />
          </CredentialIconAction>
        ) : null}
        <CredentialIconAction
          disabled={props.busy || props.connection.pending_validation || props.isDefault}
          label={props.connection.pending_validation
            ? t("settings.customModels.pendingCannotDefault")
            : props.isDefault
              ? "当前默认账号"
              : "设为默认账号"}
          onClick={props.onSetDefault}
        >
          <Star
            className={props.isDefault
              ? "size-4 fill-yellow-400 text-yellow-500"
              : "size-4"}
          />
        </CredentialIconAction>
        <CredentialIconAction label="编辑凭据" onClick={props.onEdit}>
          <KeyRound className="size-4" />
        </CredentialIconAction>
        <CredentialIconAction
          disabled={props.busy}
          label="删除凭据"
          onClick={props.onDelete}
        >
          <Trash2 className="size-4 text-destructive" />
        </CredentialIconAction>
      </CardFooter>
    </Card>
  )
}

// CredentialIconActionProps defines one compact card action whose text is delegated to a tooltip.
// CredentialIconActionProps 定义一个将文字交给提示框的紧凑卡片操作。
interface CredentialIconActionProps {
  // children contains the sole visible icon.
  // children 包含唯一可见图标。
  children: React.ReactNode
  // disabled prevents duplicate operations.
  // disabled 防止重复操作。
  disabled?: boolean
  // label is shared by the accessible name and tooltip.
  // label 同时用于可访问名称与提示框。
  label: string
  // onClick executes the page-owned operation.
  // onClick 执行页面拥有的操作。
  onClick: () => void
}

// CredentialIconAction renders the shared icon-only account action.
// CredentialIconAction 渲染共享的纯图标账号操作。
function CredentialIconAction(props: CredentialIconActionProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="inline-flex">
          <Button
            aria-label={props.label}
            disabled={props.disabled}
            onClick={props.onClick}
            size="icon-sm"
            type="button"
            variant="outline"
          >
            {props.children}
          </Button>
        </span>
      </TooltipTrigger>
      <TooltipContent side="top">{props.label}</TooltipContent>
    </Tooltip>
  )
}

// PublicAiDeleteConnectionDialog confirms destructive removal and requires an explicit replacement default.
// PublicAiDeleteConnectionDialog 确认破坏性移除，并要求显式选择替代默认账号。
function PublicAiDeleteConnectionDialog(props: {
  // Connection selected for removal.
  // 选中待移除的连接。
  connection: PublicAiConnection | null
  // All configured connections used to derive same-provider replacements.
  // 用于派生同供应商替代项的全部已配置连接。
  connections: PublicAiConnection[]
  // Whether the selected connection is the provider default.
  // 选中连接是否为供应商默认账号。
  isProviderDefault: boolean
  // Dialog visibility.
  // 弹窗可见性。
  open: boolean
  // Updates dialog visibility.
  // 更新弹窗可见性。
  onOpenChange: (open: boolean) => void
  // Receives the authoritative snapshot after removal.
  // 移除后接收权威快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const replacements = props.connection
    ? props.connections.filter((connection) => (
        connection.provider_id === props.connection?.provider_id
        && connection.id !== props.connection.id
      ))
    : []
  const [replacementId, setReplacementId] = React.useState("")
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)

  React.useEffect(() => {
    if (!props.open) {
      return
    }
    setReplacementId(replacements[0]?.id ?? "")
    setError(null)
  }, [props.open, props.connection?.id])

  // remove submits the connection and replacement choice as one atomic backend transaction.
  // remove 把连接与替代选择作为一个原子后端事务提交。
  const remove = React.useCallback(async () => {
    if (!props.connection) {
      return
    }
    if (props.isProviderDefault && replacements.length > 0 && !replacementId) {
      setError("删除默认账号前必须选择替代默认账号。")
      return
    }
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await removePublicAiConnection({
        connectionId: props.connection.id,
        replacementDefaultConnectionId:
          props.isProviderDefault && replacements.length > 0 ? replacementId : undefined,
      }))
      props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [props, replacementId, replacements.length])

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>删除模型账号</DialogTitle>
          <DialogDescription>
            将删除“{props.connection?.display_name}”及其系统凭据，此操作不可撤销。
          </DialogDescription>
        </DialogHeader>
        {props.isProviderDefault && replacements.length > 0 ? (
          <div className="settings-field">
            <span>替代默认账号</span>
            <Select onValueChange={setReplacementId} value={replacementId}>
              <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
              {replacements.map((connection) => (
                <SelectItem key={connection.id} value={connection.id}>{connection.display_name}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </div>
        ) : null}
        {error ? <p className="settings-error-text">{error}</p> : null}
        <DialogFooter>
          <button className="skill-action-button secondary" onClick={() => props.onOpenChange(false)} type="button">
            取消
          </button>
          <button className="skill-action-button secondary danger" disabled={saving} onClick={() => void remove()} type="button">
            {saving ? "删除中…" : "确认删除"}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// AccountUsageLine renders explicit unsupported, unavailable, or observed account usage.
// AccountUsageLine 渲染明确的不支持、不可用或已观测账号用量。
function AccountUsageLine(props: {
  // Selected credential profile determines whether lookup exists.
  // 选定凭据配置档决定是否存在查询能力。
  credentialProfile?: PublicAiCredentialProfile
  // Latest memory-only usage observation.
  // 最新的仅内存用量观测。
  usage?: PublicAiAccountUsage
}) {
  if (props.credentialProfile?.account_usage_query === "unsupported") {
    return <span>该凭据不支持账号用量查询</span>
  }
  if (!props.usage) {
    return <span>用量尚未查询</span>
  }
  if (props.usage.availability === "temporarily_unavailable") {
    return <span>{props.usage.refresh_message ?? "账号用量暂时不可用"}</span>
  }
  if (!Array.isArray(props.usage.quota_windows)) {
    return <span className="text-destructive">账号用量响应格式无效，请重新刷新</span>
  }
  return (
    <div className="settings-modal-list">
      <span>
        {props.usage.plan_name ? `${props.usage.plan_name} · ` : ""}
        {props.usage.remaining_amount !== undefined
          ? `剩余 ${props.usage.remaining_amount}${props.usage.currency ? ` ${props.usage.currency}` : ""}`
          : props.usage.quota_windows.length === 0 ? "该供应商不提供套餐额度窗口" : "套餐额度窗口"}
        {props.usage.used_amount !== undefined
          ? ` · 已用 ${props.usage.used_amount}${props.usage.currency ? ` ${props.usage.currency}` : ""}`
          : ""}
      </span>
      {props.usage.quota_windows.map((window) => (
        <span key={window.id}>
          {window.bucket_name ? `${window.bucket_name} · ` : ""}
          {window.display_name}：
          {!window.available
            ? "当前套餐未包含"
            : window.unlimited
              ? "无限"
              : window.total !== undefined && window.total !== "0"
                ? `${window.remaining ?? "未知"} / ${window.total} ${window.unit}${window.remaining_percent !== undefined ? ` · ${window.remaining_percent}%` : ""}`
                : window.remaining_percent !== undefined
                  ? `剩余 ${window.remaining_percent}%`
                  : `${window.remaining ?? "未知"} / ${window.total ?? "未知"} ${window.unit}`}
        </span>
      ))}
    </div>
  )
}

// PublicAiConnectionDialog creates or edits one credential-scoped connection.
// PublicAiConnectionDialog 创建或编辑一个凭据作用域连接。
function PublicAiConnectionDialog(props: {
  // Existing connection for edit mode.
  // 编辑模式下的现有连接。
  connection: PublicAiConnection | null
  // Initial provider used only when creating a new connection from a pinned entry.
  // 仅在从固定入口创建新连接时使用的初始供应商。
  initialProviderId: string
  // Dialog visibility.
  // 弹窗可见性。
  open: boolean
  // Updates dialog visibility.
  // 更新弹窗可见性。
  onOpenChange: (open: boolean) => void
  // Registered provider descriptors.
  // 已注册供应商描述。
  providers: PublicAiProviderDescriptor[]
  // Receives the updated settings snapshot.
  // 接收更新后的设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  const [providerId, setProviderId] = React.useState("")
  const [entryId, setEntryId] = React.useState("")
  const [credentialProfileId, setCredentialProfileId] = React.useState("")
  const [credentialMode, setCredentialMode] = React.useState<CredentialMode>("token")
  const [displayName, setDisplayName] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [managementKey, setManagementKey] = React.useState("")
  const [serviceOriginOverride, setServiceOriginOverride] = React.useState("")
  const [removeManagementKey, setRemoveManagementKey] = React.useState(false)
  const [oauthClientId, setOauthClientId] = React.useState("")
  const [oauthClientSecret, setOauthClientSecret] = React.useState("")
  const [oauthProjectId, setOauthProjectId] = React.useState("")
  const [oauthTransaction, setOauthTransaction] = React.useState<PublicAiOAuthTransaction | null>(null)
  const [oauthUserCode, setOauthUserCode] = React.useState<string | null>(null)
  const [pendingAuthorization, setPendingAuthorization] = React.useState<{
    transactionId: string
    providerId: string
    serviceEntryId: string
    credentialProfileId: string
    displayName: string
    connectionId?: string
  } | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)

  const provider = props.providers.find((item) => item.id === providerId) ?? props.providers[0]
  const entry = provider?.service_entries.find((item) => item.id === entryId) ?? provider?.service_entries[0]
  const credentialProfile = entry?.credential_profiles.find((item) => item.id === credentialProfileId)
    ?? entry?.credential_profiles[0]
  const registeredOauthProvider = credentialProfile?.authorization?.kind === "registered_oauth"
    ? credentialProfile.authorization.provider
    : null
  const isProviderToolEntry = entry?.kind === "provider_tools"
  const supportsServiceOriginOverride = entry?.service_origin_override_policy !== undefined

  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const existingProvider = props.connection
      ? props.providers.find((item) => item.id === props.connection?.provider_id)
      : props.providers.find((item) => item.id === props.initialProviderId) ?? props.providers[0]
    const existingEntry = props.connection
      ? existingProvider?.service_entries.find((item) => item.id === props.connection?.service_entry_id)
      : existingProvider?.service_entries[0]
    setProviderId(existingProvider?.id ?? "")
    setEntryId(existingEntry?.id ?? "")
    const existingProfile = props.connection
      ? existingEntry?.credential_profiles.find((item) => item.id === props.connection?.credential_profile_id)
      : existingEntry?.credential_profiles[0]
    setCredentialProfileId(existingProfile?.id ?? "")
    setCredentialMode(existingProfile?.credential_mode ?? props.connection?.credential_mode ?? "token")
    setDisplayName(props.connection?.display_name ?? "")
    setSecret("")
    setManagementKey("")
    setServiceOriginOverride(props.connection?.service_origin_override ?? "")
    setRemoveManagementKey(false)
    setOauthClientId("")
    setOauthClientSecret("")
    setOauthProjectId("")
    setOauthTransaction(null)
    setOauthUserCode(null)
    setPendingAuthorization(null)
    setError(null)
  }, [props.connection, props.initialProviderId, props.open, props.providers])

  React.useEffect(() => {
    if (!provider) {
      return
    }
    const selectedEntry = provider.service_entries.find((item) => item.id === entryId)
    if (!selectedEntry) {
      setEntryId(provider.service_entries[0]?.id ?? "")
    }
  }, [entryId, provider])

  React.useEffect(() => {
    if (!entry) {
      return
    }
    const selectedProfile = entry.credential_profiles.find((item) => item.id === credentialProfileId)
    if (!selectedProfile) {
      const firstProfile = entry.credential_profiles[0]
      setCredentialProfileId(firstProfile?.id ?? "")
      setCredentialMode(firstProfile?.credential_mode ?? "token")
    } else if (selectedProfile.credential_mode !== credentialMode) {
      setCredentialMode(selectedProfile.credential_mode)
    }
  }, [credentialMode, credentialProfileId, entry])

  React.useEffect(() => {
    if (!pendingAuthorization) {
      return
    }
    let disposed = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      try {
        const transaction = await getPublicAiOAuthStatus(pendingAuthorization.transactionId)
        if (disposed) {
          return
        }
        setOauthTransaction(transaction)
        if (transaction.phase === "succeeded") {
          const next = await commitPublicAiOAuthConnection(pendingAuthorization)
          if (!disposed) {
            setPendingAuthorization(null)
            props.onSaved(next)
          }
          return
        }
        if (transaction.phase === "failed" || transaction.phase === "cancelled") {
          setPendingAuthorization(null)
          return
        }
        timer = setTimeout(() => void poll(), 750)
      } catch (reason) {
        if (!disposed) {
          setPendingAuthorization(null)
          setError(errorMessage(reason))
        }
      }
    }
    void poll()
    return () => {
      disposed = true
      if (timer !== undefined) {
        clearTimeout(timer)
      }
    }
  }, [pendingAuthorization, props.onSaved])

  // save validates the visible form and delegates secrets to the host-owned credential store.
  // save 校验可见表单并把秘密委托给宿主持有的凭据仓库。
  const save = React.useCallback(async () => {
    const requiresToken = credentialMode === "token"
      && (
        !props.connection
        || props.connection.credential_mode !== "token"
        || props.connection.credential_profile_id !== credentialProfile?.id
      )
    if (
      !provider
      || !entry
      || !credentialProfile
      || !displayName.trim()
      || (requiresToken && !secret)
    ) {
      setError("请完整填写连接信息。")
      return
    }
    setSaving(true)
    setError(null)
    try {
      if (credentialMode === "auth") {
        if (!credentialProfile.authorization) {
          throw new Error("The selected credential profile does not declare an authorization flow.")
        }
        if (
          (registeredOauthProvider === "google_code_assist" || registeredOauthProvider === "google_gemini")
          && !oauthClientId.trim()
        ) {
          throw new Error("Google OAuth requires a desktop OAuth client id.")
        }
        if (registeredOauthProvider === "google_gemini" && !oauthProjectId.trim()) {
          throw new Error("Public Gemini OAuth requires a Google Cloud project id.")
        }
        const challenge = await startPublicAiOAuth({
          providerId: provider.id,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          client: {
            ...(oauthClientId.trim() ? { clientId: oauthClientId.trim() } : {}),
            ...(oauthClientSecret ? { clientSecret: oauthClientSecret } : {}),
            ...(oauthProjectId.trim() ? { projectId: oauthProjectId.trim() } : {}),
          },
        })
        setOauthTransaction(challenge.transaction)
        setOauthUserCode(challenge.userCode ?? null)
        setPendingAuthorization({
          transactionId: challenge.transaction.transactionId,
          providerId: provider.id,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          displayName,
          ...(props.connection ? { connectionId: props.connection.id } : {}),
        })
        return
      }
      const next = props.connection
        ? await updatePublicAiConnection({
          connectionId: props.connection.id,
          displayName,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          credentialMode,
          ...(secret ? { secret } : {}),
          ...(managementKey ? { managementKey } : {}),
          ...(removeManagementKey ? { removeManagementKey: true } : {}),
          ...(supportsServiceOriginOverride
            ? { serviceOriginOverride: serviceOriginOverride.trim() }
            : {}),
        })
        : await addPublicAiConnection({
          providerId: provider.id,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          credentialMode,
          displayName,
          secret,
          ...(managementKey ? { managementKey } : {}),
          ...(supportsServiceOriginOverride && serviceOriginOverride.trim()
            ? { serviceOriginOverride: serviceOriginOverride.trim() }
            : {}),
        })
      props.onSaved(next)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [
    credentialMode,
    credentialProfile,
    displayName,
    entry,
    isProviderToolEntry,
    oauthClientId,
    oauthClientSecret,
    oauthProjectId,
    props,
    provider,
    managementKey,
    removeManagementKey,
    registeredOauthProvider,
    secret,
    serviceOriginOverride,
    supportsServiceOriginOverride,
  ])

  // cancelAuthorization cancels the exact active browser transaction.
  // cancelAuthorization 取消精确的活动浏览器事务。
  const cancelAuthorization = React.useCallback(async () => {
    if (!pendingAuthorization) {
      return
    }
    try {
      setOauthTransaction(await cancelPublicAiOAuth(pendingAuthorization.transactionId))
      setPendingAuthorization(null)
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }, [pendingAuthorization])

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open && pendingAuthorization) {
          void cancelAuthorization()
        }
        props.onOpenChange(open)
      }}
    >
      <DialogContent scrollMode="body-scroll" size="lg">
        <DialogHeader>
          <DialogTitle>
            {props.connection
              ? isProviderToolEntry ? "编辑工具服务凭据" : "编辑模型账号"
              : isProviderToolEntry ? "添加工具服务凭据" : "添加模型账号"}
          </DialogTitle>
          <DialogDescription>
            {isProviderToolEntry
              ? "该凭据只用于所选供应商的原生 AI 工具，不会创建虚假的对话模型。"
              : "协议绑定在当前账号上，模型调用期间不会切换协议或账号。"}
          </DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>供应商</span>
            <span className="provider-credential-select">
              <ProviderIcon category={provider?.category} providerId={provider?.id} />
              <Select
                disabled={props.connection !== null}
                onValueChange={(value) => {
                  setProviderId(value)
                  setServiceOriginOverride("")
                }}
                value={provider?.id ?? ""}
              >
                <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {props.providers.map((item) => <SelectItem key={item.id} value={item.id}>{item.display_name}</SelectItem>)}
                </SelectContent>
              </Select>
            </span>
          </label>
          <label className="settings-form-field">
            <span>服务入口</span>
            {provider?.credential_registration === "mini_max_regional_api_key"
              && credentialMode === "token" ? (
              <strong>保存时自动识别区域</strong>
            ) : (
              <Select
                onValueChange={(value) => {
                  setEntryId(value)
                  if (value !== entry?.id) {
                    setServiceOriginOverride("")
                  }
                }}
                value={entry?.id ?? ""}
              >
                <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {(provider?.service_entries ?? []).map((item) => <SelectItem key={item.id} value={item.id}>{item.display_name}</SelectItem>)}
                </SelectContent>
              </Select>
            )}
          </label>
          <label className="settings-form-field">
            <span>凭据类型</span>
            <Select
              onValueChange={(value) => {
                const nextProfile = entry?.credential_profiles.find((item) => item.id === value)
                setCredentialProfileId(value)
                setCredentialMode(nextProfile?.credential_mode ?? "token")
              }}
              value={credentialProfile?.id ?? ""}
            >
              <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
              {(entry?.credential_profiles ?? []).map((profile) => (
                <SelectItem key={profile.id} value={profile.id}>{profile.display_name}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </label>
          <label className="settings-form-field">
            <span>账号显示名</span>
            <Input onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
          </label>
          {credentialMode === "token" ? (
            <label className="settings-form-field">
              <span>{props.connection ? "替换 Token（留空则不变）" : "API Token"}</span>
              <Input
                autoComplete="off"
                onChange={(event) => setSecret(event.target.value)}
                type="password"
                value={secret}
              />
            </label>
          ) : null}
          {supportsServiceOriginOverride ? (
            <label className="settings-form-field">
              <span>{t("settings.providerConnection.serviceOrigin")}</span>
              <Input
                autoComplete="off"
                onChange={(event) => setServiceOriginOverride(event.target.value)}
                placeholder={entry?.service_origin_override_policy?.example_origin ?? "https://workspace.example.com"}
                value={serviceOriginOverride}
              />
              <small>{t("settings.providerConnection.serviceOriginDescription")}</small>
            </label>
          ) : null}
          {credentialProfile
            && usageCapabilityRequiresManagementKey(credentialProfile.account_usage_query) ? (
            <div className="settings-form-field">
              <span>管理密钥（可选）</span>
              <Input
                autoComplete="off"
                onChange={(event) => setManagementKey(event.target.value)}
                placeholder={props.connection?.has_management_key ? "留空保持现有管理密钥" : ""}
                type="password"
                value={managementKey}
              />
              <small>仅用于该账号的用量查询，不用于模型调用。</small>
              {props.connection?.has_management_key ? (
                <label className="settings-inline-checkbox">
                  <Switch
                    checked={removeManagementKey}
                    onCheckedChange={setRemoveManagementKey}
                  />
                  <span>移除现有管理密钥</span>
                </label>
              ) : null}
            </div>
          ) : null}
          {credentialMode === "auth"
            && (registeredOauthProvider === "google_code_assist" || registeredOauthProvider === "google_gemini") ? (
            <>
              <label className="settings-form-field">
                <span>Google OAuth Client ID</span>
                <Input onChange={(event) => setOauthClientId(event.target.value)} value={oauthClientId} />
              </label>
              <label className="settings-form-field">
                <span>Google OAuth Client Secret（可选）</span>
                <Input
                  autoComplete="off"
                  onChange={(event) => setOauthClientSecret(event.target.value)}
                  type="password"
                  value={oauthClientSecret}
                />
              </label>
              {registeredOauthProvider === "google_gemini" ? (
                <label className="settings-form-field">
                  <span>Google Cloud Project ID</span>
                  <Input
                    onChange={(event) => setOauthProjectId(event.target.value)}
                    value={oauthProjectId}
                  />
                </label>
              ) : null}
            </>
          ) : null}
        </div>
        {oauthTransaction ? (
          <div className="settings-muted-line">
            <span>
              授权状态：{oauthPhaseDisplayName(oauthTransaction.phase)}
              {oauthTransaction.message ? ` · ${oauthTransaction.message}` : ""}
            </span>
            {oauthUserCode ? <strong>授权码：{oauthUserCode}</strong> : null}
          </div>
        ) : null}
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        <DialogFooter>
          <button
            className="skill-action-button secondary"
            onClick={() => pendingAuthorization ? void cancelAuthorization() : props.onOpenChange(false)}
            type="button"
          >
            {pendingAuthorization ? "取消授权" : "取消"}
          </button>
          <button className="skill-action-button" disabled={saving || pendingAuthorization !== null} onClick={() => void save()} type="button">
            {pendingAuthorization
              ? "等待浏览器授权"
              : saving
                ? "保存并同步…"
                : credentialMode === "auth"
                  ? props.connection
                    ? "重新授权并保存"
                    : "打开浏览器授权"
                  : "保存并同步"}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// providerForConnection resolves the exact provider owner.
// providerForConnection 解析精确供应商归属。
function providerForConnection(
  snapshot: PublicAiSettingsSnapshot,
  connection: PublicAiConnection,
): PublicAiProviderDescriptor | undefined {
  return snapshot.providers.find((provider) => provider.id === connection.provider_id)
}

// entryForConnection resolves the exact service entry.
// entryForConnection 解析精确服务入口。
function entryForConnection(
  provider: PublicAiProviderDescriptor | undefined,
  connection: PublicAiConnection,
): PublicAiServiceEntry | undefined {
  return provider?.service_entries.find((entry) => entry.id === connection.service_entry_id)
}

// credentialProfileForConnection resolves the exact acquisition and billing profile.
// credentialProfileForConnection 解析精确的凭据获取及计费配置档。
function credentialProfileForConnection(
  entry: PublicAiServiceEntry | undefined,
  connection: PublicAiConnection,
): PublicAiCredentialProfile | undefined {
  return entry?.credential_profiles.find((profile) => profile.id === connection.credential_profile_id)
}

// protocolDisplayName maps registered profile identifiers to user-facing labels.
// protocolDisplayName 把已注册 Profile 标识映射为用户可见标签。
function protocolDisplayName(protocolId: string): string {
  const labels: Record<string, string> = {
    "anthropic-messages.v1": "Anthropic Messages",
    "anthropic-messages-video.v1": "Anthropic Messages + Video",
    "claude-code-messages.v1": "Claude Code Messages",
    "openai-chat-completions.v1": "OpenAI Chat Completions",
    "openai-codex-responses.v1": "OpenAI Codex Responses",
    "openai-responses.v1": "OpenAI Responses",
  }
  return labels[protocolId] ?? protocolId
}

// oauthPhaseDisplayName maps exact OAuth phases to user-facing progress labels.
// oauthPhaseDisplayName 将精确 OAuth 阶段映射为用户可见进度标签。
function oauthPhaseDisplayName(phase: PublicAiOAuthTransaction["phase"]): string {
  const labels: Record<PublicAiOAuthTransaction["phase"], string> = {
    waiting_for_user_authorization: "等待用户确认授权",
    waiting_for_callback: "等待浏览器回调",
    finalizing_credential: "正在完成凭据保存",
    succeeded: "授权成功",
    failed: "授权失败",
    cancelled: "已取消",
  }
  return labels[phase]
}

// errorMessage normalizes unknown service failures for inline display.
// errorMessage 归一化未知服务失败以供行内展示。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
