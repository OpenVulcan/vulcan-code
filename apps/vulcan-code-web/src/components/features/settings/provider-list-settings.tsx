import { WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react"
import { Boxes, KeyRound, ListChecks, Pencil, RefreshCw, Star, Trash2 } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { PageEmptyState } from "@/components/page"
import { CredentialModelAccessDialog } from "@/components/features/settings/credential-model-access-dialog"
import {
  localizedProviderDescription,
  localizedProviderName,
  localizedServiceEntryPresentation,
} from "@/components/features/settings/provider-presentation"
import { orderCredentialDirectoryProviders } from "@/components/features/settings/provider-ordering"
import {
  CredentialUsageInlineFacts,
  CredentialUsageQuotaPanel,
  useCredentialUsageOverview,
} from "@/components/public-ai/credential-usage-overview"
import { usageCapabilityRequiresManagementKey } from "@/components/public-ai/credential-usage-capability"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
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
import { useI18n } from "@/i18n"
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice"
import {
  addPublicAiConnection,
  cancelPublicAiOAuth,
  commitPublicAiOAuthConnection,
  getPublicAiOAuthStatus,
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
  removePublicAiConnection,
  savePublicAiConnectionEnabled,
  savePublicAiDefaultConnection,
  startPublicAiOAuth,
  syncPublicAiConnection,
  updatePublicAiConnection,
} from "@/service/settings-actions-service"
import type {
  CredentialMode,
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiCredentialProfile,
  PublicAiModelDirectoryKind,
  PublicAiOAuthTransaction,
  PublicAiProviderDescriptor,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"
import { errorMessage } from "@/lib/display-format";

// CredentialManagementArea renders credential-only account management and runtime metadata.
// CredentialManagementArea 渲染仅凭据账号管理与运行时元数据。
export function CredentialManagementArea() {
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [busyConnectionId, setBusyConnectionId] = React.useState<string | null>(null)
  const [editingConnection, setEditingConnection] = React.useState<PublicAiConnection | null>(null)
  const [deletingConnection, setDeletingConnection] = React.useState<PublicAiConnection | null>(null)
  const [modelAccessConnection, setModelAccessConnection] = React.useState<PublicAiConnection | null>(null)
  const [usageByConnection, setUsageByConnection] = React.useState<Record<string, PublicAiAccountUsage>>({})
  const [activeProviderId, setActiveProviderId] = React.useState<string>("")
  const [creatingProviderId, setCreatingProviderId] = React.useState<string>("")
  // pickerOpen keeps provider-scoped credential creation local to this settings page.
  // pickerOpen 将供应商作用域的凭据创建状态保留在当前设置页面内。
  const [pickerOpen, setPickerOpen] = React.useState(false)
  // Serialize account writes and usage reads so stale snapshots cannot overwrite newer choices.
  // 串行执行账号写入与用量读取，避免旧快照覆盖更新的选择。
  const accountActionRunningRef = React.useRef(false)
  // Directory retries are read-only, but must not race one another or account mutations.
  // 目录重试虽然只读，也不能彼此竞争或与账号修改竞争。
  const directoryLoadingRef = React.useRef(false)
  const [directoryLoading, setDirectoryLoading] = React.useState(false)
  const { t } = useI18n()

  // refresh loads the authoritative persisted graph and current process-local catalogs.
  // refresh 加载权威持久化连接图与当前进程内目录。
  const refresh = React.useCallback(async () => {
    if (directoryLoadingRef.current || accountActionRunningRef.current) return
    directoryLoadingRef.current = true
    setDirectoryLoading(true)
    try {
      setError(null)
      const next = await loadPublicAiSettings()
      setSnapshot(next)
      setUsageByConnection(next.usageCache)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      directoryLoadingRef.current = false
      setDirectoryLoading(false)
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
    if (accountActionRunningRef.current) return
    accountActionRunningRef.current = true
    setBusyConnectionId(connectionId)
    setError(null)
    try {
      setSnapshot(await action())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      accountActionRunningRef.current = false
      setBusyConnectionId(null)
    }
  }, [])

  // refreshUsage queries and displays one account-scoped usage observation.
  // refreshUsage 查询并展示一个账号作用域用量观测。
  const refreshUsage = React.useCallback(async (connectionId: string) => {
    if (accountActionRunningRef.current) return
    accountActionRunningRef.current = true
    setBusyConnectionId(connectionId)
    setError(null)
    try {
      const usage = await queryPublicAiConnectionUsage(connectionId)
      setUsageByConnection((current) => ({ ...current, [connectionId]: usage }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      accountActionRunningRef.current = false
      setBusyConnectionId(null)
    }
  }, [])

  // directoryProviders keeps every registered provider visible in the requested two-level order.
  // directoryProviders 按指定两级顺序保持所有已注册供应商可见。
  const directoryProviders = React.useMemo(() => {
    if (!snapshot) {
      return []
    }
    return orderCredentialDirectoryProviders(snapshot)
  }, [snapshot])
  // visibleConnections applies the exact left-menu provider filter.
  // visibleConnections 应用精确的左侧菜单供应商筛选。
  const visibleConnections = snapshot?.connections.filter((connection) => (
    connection.provider_id === activeProviderId
  )) ?? []
  // activeProvider is the exact provider selected by the fixed directory.
  // activeProvider 是固定目录选中的精确供应商。
  const activeProvider = directoryProviders.find((provider) => provider.id === activeProviderId)
  // Select the first ordered provider when the current provider is absent or stale.
  // 当前供应商缺失或过期时选择排序后的第一个供应商。
  React.useEffect(() => {
    if (!activeProviderId || !directoryProviders.some((provider) => provider.id === activeProviderId)) {
      setActiveProviderId(directoryProviders[0]?.id ?? "")
    }
  }, [activeProviderId, directoryProviders])

  const modelAccessProvider = snapshot && modelAccessConnection
    ? providerForConnection(snapshot, modelAccessConnection)
    : undefined
  const modelAccessEntry = modelAccessConnection
    ? entryForConnection(modelAccessProvider, modelAccessConnection)
    : undefined

  return (
    <WorkspaceDirectoryLayout contentClassName="credential-management-surface" navigation={<WorkspaceDirectoryMenu ariaLabel={t("settings.credentials.categoryAria")} heading={t("settings.credentials.categoryAria")}>
        {directoryProviders.map((provider) => (
          <Button
            aria-current={activeProviderId === provider.id ? "page" : undefined}
            className={`workspace-tool-panel-menu-row ${activeProviderId === provider.id ? "active" : ""}`}
            key={provider.id}
            onClick={() => setActiveProviderId(provider.id)}
            type="button"
            variant="ghost"
          >
            <ProviderIcon category={provider.category} className="size-5" providerId={provider.id} />
            <span className="settings-directory-label" title={localizedProviderName(provider.id, provider.display_name, t)}>{localizedProviderName(provider.id, provider.display_name, t)}</span>
            <small>{snapshot?.connections.filter((connection) => connection.provider_id === provider.id).length ?? 0}</small>
          </Button>
        ))}
        {/* The provider directory streams in with the credential snapshot. / 供应商目录随凭据快照流式载入。 */}
        {!snapshot && !error ? <StreamPlaceholder decorative shape="rows" count={6} /> : null}
      </WorkspaceDirectoryMenu>}>
        {activeProvider ? (
          <div className="sticky top-0 z-20 mb-4 flex flex-wrap items-center justify-between gap-3 border-b bg-background/95 py-3 backdrop-blur supports-[backdrop-filter]:bg-background/80">
            <div className="flex min-w-0 items-center gap-3">
              <ProviderIcon
                category={activeProvider.category}
                className="size-8 shrink-0"
                providerId={activeProvider.id}
              />
              <div className="min-w-0">
                <h3 className="font-semibold">
                  {localizedProviderName(activeProvider.id, activeProvider.display_name, t)}
                </h3>
                <p className="text-sm text-muted-foreground">
                  {localizedProviderDescription(activeProvider.id, t)}
                </p>
              </div>
            </div>
            <Button
              disabled={busyConnectionId !== null || directoryLoading}
              onClick={() => {
                setCreatingProviderId(activeProvider.id)
                setPickerOpen(true)
              }}
              size="sm"
              type="button"
            >
              <KeyRound className="mr-1 size-4" />
               {t("settings.providerOnboarding.addCredential")}
            </Button>
          </div>
        ) : null}
        {error ? <SettingsErrorNotice className="settings-error-text" detail={error} message={t("settings.providers.thatDidnTFinish")} /> : null}
        {!snapshot && !error ? (
          <StreamPlaceholder fill label={t("settings.credentials.connecting")} shape="cards" />
        ) : !snapshot ? (
          <article className="settings-card">
            <strong>{t("settings.providers.credentialDirectoryCouldNot")}</strong>
            <Button disabled={directoryLoading} onClick={() => void refresh()} type="button" variant="outline">{t("settings.providers.reload")}</Button>
          </article>
        ) : visibleConnections.length > 0 ? (
          <div className="credential-management-list">
            {visibleConnections.map((connection) => {
              const provider = providerForConnection(snapshot, connection)
              const entry = entryForConnection(provider, connection)
              const credentialProfile = credentialProfileForConnection(entry, connection)
              const usage = usageByConnection[connection.id] ?? snapshot.usageCache[connection.id]
              // modelDirectoryKind is the backend-resolved source policy for this exact credential.
              // modelDirectoryKind 是后端为此精确凭据解析的来源策略。
              const modelDirectoryKind = snapshot.connectionModelAccessPolicyRecords
                .find((record) => record.connectionId === connection.id)
                ?.directoryKind
              // Provider-tool entries do not own a model catalog, so historical model-sync errors are irrelevant.
              // 供应商工具入口不拥有模型目录，因此历史模型同步错误与其无关。
              const catalogError = !connection.enabled || entry?.kind === "provider_tools"
                ? undefined
                : snapshot.catalogErrors[connection.id]
              const isDefault = snapshot.providerDefaults[connection.provider_id] === connection.id
              return (
                <CredentialAccountCard
                  busy={busyConnectionId !== null}
                  catalogError={catalogError}
                  connection={connection}
                  credentialProfile={credentialProfile}
                  entry={entry}
                  isDefault={isDefault}
                  key={connection.id}
                  modelDirectoryKind={modelDirectoryKind}
                  onDelete={() => setDeletingConnection(connection)}
                  onEdit={() => setEditingConnection(connection)}
                  onEnabledChange={(enabled) => void runConnectionAction(
                    connection.id,
                    () => savePublicAiConnectionEnabled({ connectionId: connection.id, enabled }),
                  )}
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
        ) : activeProviderId ? (
          <PageEmptyState
            description={t("settings.credentials.emptyProviderDescription")}
            icon={<KeyRound />}
            title={t("settings.credentials.emptyProvider")}
          />
        ) : (
          <PageEmptyState
            action={<Button disabled={directoryLoading} onClick={() => void refresh()} type="button" variant="outline"><RefreshCw className="size-4" />{t("settings.providers.reloadDirectory")}</Button>}
            description={t("settings.providers.loadTheProviderDirectory")}
            icon={<Boxes />}
            title={t("settings.providers.theProviderDirectoryIs")}
          />
        )}
        <PublicAiConnectionDialog
          connection={null}
          initialProviderId={creatingProviderId}
          lockedProviderId={creatingProviderId}
          onOpenChange={(open) => {
            setPickerOpen(open)
            if (!open) {
              setCreatingProviderId("")
            }
          }}
          onSaved={(next) => {
            setSnapshot(next)
            setCreatingProviderId("")
            setPickerOpen(false)
          }}
          open={pickerOpen}
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
        lockedProviderId={editingConnection?.provider_id ?? ""}
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
    </WorkspaceDirectoryLayout>
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
  // modelDirectoryKind is the backend-resolved fixed, dynamic, manual, or unavailable catalog policy.
  // modelDirectoryKind 是后端解析的固定、动态、手工或不可用目录策略。
  modelDirectoryKind?: PublicAiModelDirectoryKind
  // onDelete opens the destructive confirmation.
  // onDelete 打开破坏性确认。
  onDelete: () => void
  // onEdit opens exact credential editing.
  // onEdit 打开精确凭据编辑。
  onEdit: () => void
  // onEnabledChange persists whether this credential participates in runtime work.
  // onEnabledChange 持久化此凭据是否参与运行时工作。
  onEnabledChange: (enabled: boolean) => void
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

// CredentialAccountCard separates account identity, authoritative usage, and management actions in one row.
// CredentialAccountCard 在单行卡片中分离账号身份、权威用量和管理操作。
// Parameters: props supplies one credential and its guarded callbacks; returns its account card without changing admission rules.
// 参数：props 提供一个凭据及受保护回调；返回账号卡片，不改变准入规则。
function CredentialAccountCard(props: CredentialAccountCardProps) {
  const { t } = useI18n()
  // usageSupported preserves the provider capability and management-key prerequisites for refreshing usage.
  // usageSupported 保留刷新用量所需的供应商能力和管理密钥前置条件。
  const usageSupported = props.credentialProfile?.account_usage_query !== "unsupported"
    && (!props.connection.usage_requires_management_key || props.connection.has_management_key)
  // providerName localizes registered built-in names while retaining unknown provider identities.
  // providerName 本地化已注册内置名称，同时保留未知供应商身份。
  const providerName = localizedProviderName(
    props.connection.provider_id,
    props.provider?.display_name ?? props.connection.provider_id,
    t,
  )
  // serviceEntryPresentation separates Alibaba product names from composable scope and region tags.
  // serviceEntryPresentation 将阿里云产品名称与可组合的账号范围、区域标签分离。
  const serviceEntryPresentation = props.entry
    ? localizedServiceEntryPresentation(props.connection.provider_id, props.entry, t)
    : { title: providerName, tags: [] }
  // usageOverview splits plan and balance into the identity column and quota into the usage column.
  // usageOverview 将套餐与余额放入身份列，额度放入用量列。
  const usageOverview = useCredentialUsageOverview({
    connection: props.connection,
    credentialProfile: props.credentialProfile,
    usage: props.usage,
  })
  return (
    <Card
      className="credential-account-card w-full gap-0 border border-border py-0 ring-0"
      size="sm"
    >
      <div className="credential-account-body">
        <CardHeader className="credential-account-identity">
          <div className="flex min-w-0 items-center gap-3">
            <span className="relative shrink-0">
              <ProviderIcon
                category={props.provider?.category}
                className="size-9"
                providerId={props.connection.provider_id}
                serviceEntryId={props.connection.service_entry_id}
              />
              {props.isDefault ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span
                      aria-label={t("settings.credentials.currentDefault")}
                      className="credential-account-default-mark"
                      role="img"
                      tabIndex={0}
                    >
                      <Star aria-hidden="true" className="size-2.5 fill-current" />
                    </span>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("settings.credentials.currentDefault")}</TooltipContent>
                </Tooltip>
              ) : null}
            </span>
            <div className="min-w-0 flex-1">
              <CardTitle className="truncate" title={props.connection.display_name}>{props.connection.display_name}</CardTitle>
              <CardDescription className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
                <span className="min-w-0 truncate" title={serviceEntryPresentation.title}>
                  {serviceEntryPresentation.title}
                </span>
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
                {!props.connection.enabled ? (
                  <Badge className="h-4 px-1.5 text-[10px]" variant="secondary">
                    {t("settings.credentials.disabledStatus")}
                  </Badge>
                ) : null}
              </CardDescription>
            </div>
          </div>
        </CardHeader>

        <CardContent className="credential-account-usage">
          <CredentialUsageQuotaPanel header={<CredentialUsageInlineFacts model={usageOverview.model} />}>
            {usageOverview.quotaContent}
          </CredentialUsageQuotaPanel>
        </CardContent>
      </div>
      {props.catalogError ? (
        <p className="credential-account-error text-xs text-destructive" role="status">
          {props.catalogError}
        </p>
      ) : null}

      <CardFooter className="credential-account-footer">
        <div className="credential-account-actions">
          <label className="credential-account-enabled">
            <Switch
              aria-label={props.connection.enabled
                ? t("settings.credentials.disable")
                : t("settings.credentials.enable")}
              checked={props.connection.enabled}
              disabled={props.busy}
              onCheckedChange={props.onEnabledChange}
            />
            <span>{t("settings.credentials.enable")}</span>
          </label>
          {props.entry?.kind !== "provider_tools" ? (
            <Button
              disabled={props.busy || !props.connection.enabled || props.connection.pending_validation}
              onClick={props.onManageModels}
              size="sm"
              type="button"
              variant="ghost"
            >
              <ListChecks aria-hidden="true" className="size-4" />
              {t("settings.credentialModels.manage")}
            </Button>
          ) : null}
          {credentialSupportsModelSynchronization(props.modelDirectoryKind) ? (
            <CredentialIconAction
              disabled={props.busy || !props.connection.enabled || props.connection.pending_validation}
              label={t("settings.credentials.syncModels")}
              onClick={props.onSync}
            >
              <Boxes className="size-4" />
            </CredentialIconAction>
          ) : null}
        </div>
        <div className="credential-account-actions">
          {usageSupported ? (
            <CredentialIconAction
              disabled={props.busy || !props.connection.enabled}
              label={t("settings.credentials.refreshUsage")}
              onClick={props.onRefreshUsage}
            >
              <RefreshCw className={`size-4 ${props.busy ? "animate-spin" : ""}`} />
            </CredentialIconAction>
          ) : null}
          <CredentialIconAction
            disabled={props.busy || !props.connection.enabled || props.connection.pending_validation || props.isDefault}
            label={props.connection.pending_validation
              ? t("settings.customModels.pendingCannotDefault")
              : props.isDefault
                 ? t("settings.credentials.currentDefault")
                 : t("settings.credentials.setDefault")}
            onClick={props.onSetDefault}
          >
            <Star
              className={props.isDefault
                ? "size-4 fill-current text-[var(--vcn-content-warning)]"
                : "size-4"}
            />
          </CredentialIconAction>
          <CredentialIconAction disabled={props.busy} label={t("settings.credentials.edit")} onClick={props.onEdit}>
            <Pencil aria-hidden="true" className="size-4" />
          </CredentialIconAction>
          <CredentialIconAction
            disabled={props.busy}
            label={t("settings.credentials.remove")}
            onClick={props.onDelete}
          >
            <Trash2 className="size-4 text-destructive" />
          </CredentialIconAction>
        </div>
      </CardFooter>
    </Card>
  )
}

// credentialSupportsModelSynchronization identifies directories whose model membership can change remotely.
// credentialSupportsModelSynchronization 标识模型成员可由远端变化的目录。
export function credentialSupportsModelSynchronization(
  directoryKind: PublicAiModelDirectoryKind | undefined,
): boolean {
  return directoryKind === "dynamic"
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
  const { t } = useI18n()
  const replacements = props.connection
    ? props.connections.filter((connection) => (
        connection.provider_id === props.connection?.provider_id
        && connection.id !== props.connection.id
      ))
    : []
  const [replacementId, setReplacementId] = React.useState("")
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)
  // Deletion owns the selected identity until the backend acknowledges success or failure.
  // 后端确认成功或失败前，删除操作持有选定身份。
  const savingRef = React.useRef(false)

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
    if (savingRef.current || !props.connection) {
      return
    }
    if (props.isProviderDefault && replacements.length > 0 && !replacementId) {
      setError(t("settings.credentials.deleteDefaultMissingReplacement"))
      return
    }
    savingRef.current = true
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
      savingRef.current = false
      setSaving(false)
    }
  }, [props, replacementId, replacements.length])

  return (
    <Dialog open={props.open} onOpenChange={(open) => { if (!savingRef.current) props.onOpenChange(open) }}>
      <DialogContent size="sm">
        <DialogHeader>
          <DialogTitle>{t("settings.credentials.deleteTitle")}</DialogTitle>
          <DialogDescription>
            {t("settings.credentials.deleteDescription").replace("{name}", props.connection?.display_name ?? "")}
          </DialogDescription>
        </DialogHeader>
        {props.isProviderDefault && replacements.length > 0 ? (
          <div className="settings-field">
            <span>{t("settings.credentials.replacementDefault")}</span>
            <Select disabled={saving} onValueChange={setReplacementId} value={replacementId}>
              <SelectTrigger aria-label={t("settings.credentials.replacementDefault")} className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>
              {replacements.map((connection) => (
                <SelectItem key={connection.id} value={connection.id}>{connection.display_name}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </div>
        ) : null}
        {error ? <p className="settings-error-text" role="alert">{error}</p> : null}
        <DialogFooter>
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">
            {t("settings.credentials.cancel")}
          </Button>
          <Button disabled={saving} onClick={() => void remove()} type="button" variant="destructive">
            {saving ? t("settings.credentials.deleting") : t("settings.credentials.confirmDelete")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// PublicAiConnectionDialog creates or edits one credential-scoped connection.
// PublicAiConnectionDialog 创建或编辑一个凭据作用域连接。
export function PublicAiConnectionDialog(props: {
  // Existing connection for edit mode.
  // 编辑模式下的现有连接。
  connection: PublicAiConnection | null
  // Initial provider used only when creating a new connection from a pinned entry.
  // 仅在从固定入口创建新连接时使用的初始供应商。
  initialProviderId: string
  // Exact provider locked by the owning provider section.
  // 由所属供应商区域锁定的精确供应商。
  lockedProviderId: string
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
  // Stable identity preserves drafts across unrelated settings snapshot refreshes.
  // 稳定身份在无关设置快照刷新时保留草稿。
  const initializedConnectionKey = React.useRef<string | null>(null)
  const savingRef = React.useRef(false)
  const cancellingRef = React.useRef(false)
  const [cancelling, setCancelling] = React.useState(false)
  const onSavedRef = React.useRef(props.onSaved)
  onSavedRef.current = props.onSaved
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

  const provider = props.providers.find((item) => item.id === providerId)
    ?? (props.lockedProviderId ? undefined : props.providers[0])
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
      initializedConnectionKey.current = null
      return
    }
    const connectionKey = JSON.stringify([props.connection?.id ?? null, props.lockedProviderId, props.initialProviderId])
    if (initializedConnectionKey.current === connectionKey) return
    initializedConnectionKey.current = connectionKey
    const existingProvider = props.connection
      ? props.providers.find((item) => item.id === props.connection?.provider_id)
      : props.providers.find((item) => item.id === props.lockedProviderId)
        ?? props.providers.find((item) => item.id === props.initialProviderId)
        ?? (props.lockedProviderId ? undefined : props.providers[0])
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
  }, [
    props.connection,
    props.initialProviderId,
    props.lockedProviderId,
    props.open,
    props.providers,
  ])

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
        if (cancellingRef.current) {
          timer = setTimeout(() => void poll(), 750)
          return
        }
        setOauthTransaction(transaction)
        if (transaction.phase === "succeeded") {
          savingRef.current = true
          setSaving(true)
          try {
            const next = await commitPublicAiOAuthConnection(pendingAuthorization)
            if (!disposed) {
              setPendingAuthorization(null)
              onSavedRef.current(next)
            }
          } finally {
            savingRef.current = false
            if (!disposed) setSaving(false)
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
  }, [pendingAuthorization])

  // save validates the visible form and delegates secrets to the host-owned credential store.
  // save 校验可见表单并把秘密委托给宿主持有的凭据仓库。
  const save = React.useCallback(async () => {
    if (savingRef.current || cancellingRef.current || pendingAuthorization) return
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
      setError(t("settings.credentials.connectionIncomplete"))
      return
    }
    savingRef.current = true
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
      savingRef.current = false
      setSaving(false)
    }
  }, [
    credentialMode,
    pendingAuthorization,
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
  const cancelAuthorization = React.useCallback(async (closeAfterCancel = false) => {
    if (!pendingAuthorization || savingRef.current || cancellingRef.current) {
      return
    }
    cancellingRef.current = true
    setCancelling(true)
    try {
      setOauthTransaction(await cancelPublicAiOAuth(pendingAuthorization.transactionId))
      setPendingAuthorization(null)
      if (closeAfterCancel) props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      cancellingRef.current = false
      setCancelling(false)
    }
  }, [pendingAuthorization, props.onOpenChange])

  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (savingRef.current || cancellingRef.current) return
        if (!open && pendingAuthorization) {
          void cancelAuthorization(true)
          return
        }
        props.onOpenChange(open)
      }}
    >
      <DialogContent className="flex min-h-0 flex-col" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>
            {props.connection
              ? isProviderToolEntry ? t("settings.credentials.editToolTitle") : t("settings.credentials.editModelTitle")
              : isProviderToolEntry ? t("settings.credentials.addToolTitle") : t("settings.credentials.addModelTitle")}
          </DialogTitle>
          <DialogDescription>
            {isProviderToolEntry
              ? t("settings.credentials.toolDescription")
              : t("settings.credentials.modelDescription")}
          </DialogDescription>
        </DialogHeader>
        <fieldset className="settings-form-grid min-h-0 flex-1 overflow-y-auto" disabled={saving || cancelling || pendingAuthorization !== null}>
          <label className="settings-form-field">
            <span>{t("settings.credentials.provider")}</span>
            <span className="provider-credential-select">
              <ProviderIcon category={provider?.category} providerId={provider?.id} />
              <Select
                disabled={props.connection !== null || props.lockedProviderId !== ""}
                onValueChange={(value) => {
                  setProviderId(value)
                  setServiceOriginOverride("")
                }}
                value={provider?.id ?? ""}
              >
                <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {props.providers.map((item) => (
                    <SelectItem key={item.id} value={item.id}>
                      {localizedProviderName(item.id, item.display_name, t)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </span>
          </label>
          <label className="settings-form-field">
            <span>{t("settings.credentials.serviceEntry")}</span>
            {provider?.credential_registration === "mini_max_regional_api_key"
              && credentialMode === "token" ? (
              <strong>{t("settings.credentials.autoRegion")}</strong>
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
            <span>{t("settings.credentials.credentialType")}</span>
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
            <span>{t("settings.credentials.displayName")}</span>
            <Input onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
          </label>
          {credentialMode === "token" ? (
            <label className="settings-form-field">
              <span>{props.connection ? t("settings.credentials.replaceToken") : t("settings.credentials.apiToken")}</span>
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
              <span>{t("settings.credentials.managementKey")}</span>
              <Input
                autoComplete="off"
                onChange={(event) => setManagementKey(event.target.value)}
                placeholder={props.connection?.has_management_key ? t("settings.credentials.managementKeyPlaceholder") : ""}
                type="password"
                value={managementKey}
              />
              <small>{t("settings.credentials.managementKeyHint")}</small>
              {props.connection?.has_management_key ? (
                <label className="settings-inline-checkbox">
                  <Switch
                    checked={removeManagementKey}
                    onCheckedChange={setRemoveManagementKey}
                  />
                  <span>{t("settings.credentials.removeManagementKey")}</span>
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
                <span>{t("settings.credentials.oauthClientSecretOptional")}</span>
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
        </fieldset>
        {oauthTransaction ? (
          <div className="settings-muted-line">
            <span>
              {t("settings.credentials.authorizationStatus")}{t("global.labelSeparator")}{oauthPhaseDisplayName(oauthTransaction.phase, t)}
              {oauthTransaction.message ? ` · ${oauthTransaction.message}` : ""}
            </span>
            {oauthUserCode ? <strong>{t("settings.credentials.authorizationCode")}{t("global.labelSeparator")}{oauthUserCode}</strong> : null}
          </div>
        ) : null}
        {error ? <p className="settings-muted-line settings-error-text" role="alert">{error}</p> : null}
        <DialogFooter>
          <Button
            disabled={saving || cancelling}
            variant="outline"
            onClick={() => pendingAuthorization ? void cancelAuthorization() : props.onOpenChange(false)}
            type="button"
          >
            {pendingAuthorization ? t("settings.credentials.cancelAuthorization") : t("settings.credentials.cancel")}
          </Button>
          <Button disabled={saving || cancelling || pendingAuthorization !== null} onClick={() => void save()} type="button">
            {pendingAuthorization
              ? t("settings.credentials.waitBrowser")
              : saving
                ? t("settings.credentials.saveSyncing")
                : credentialMode === "auth"
                  ? props.connection
                    ? t("settings.credentials.reauthorizeAndSave")
                    : t("settings.credentials.openBrowserAuthorize")
                  : t("settings.credentials.saveSync")}
          </Button>
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

// oauthPhaseDisplayName maps exact OAuth phases to user-facing progress labels.
// oauthPhaseDisplayName 将精确 OAuth 阶段映射为用户可见进度标签。
function oauthPhaseDisplayName(
  phase: PublicAiOAuthTransaction["phase"],
  t: ReturnType<typeof useI18n>["t"],
): string {
  const labels: Record<PublicAiOAuthTransaction["phase"], Parameters<typeof t>[0]> = {
    waiting_for_user_authorization: "settings.credentials.oauth.waitingAuthorization",
    waiting_for_callback: "settings.credentials.oauth.waitingCallback",
    finalizing_credential: "settings.credentials.oauth.finalizing",
    succeeded: "settings.credentials.oauth.succeeded",
    failed: "settings.credentials.oauth.failed",
    cancelled: "settings.credentials.oauth.cancelled",
  }
  return t(labels[phase])
}
