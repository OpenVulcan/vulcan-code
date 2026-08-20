import * as React from "react"
import { Boxes, KeyRound, ListChecks, RefreshCw, Star, Trash2 } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { CredentialModelAccessDialog } from "@/components/features/settings/credential-model-access-dialog"
import {
  localizedProviderDescription,
  localizedProviderName,
  localizedServiceEntryPresentation,
} from "@/components/features/settings/provider-presentation"
import { orderCredentialDirectoryProviders } from "@/components/features/settings/provider-ordering"
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
  addPublicAiConnection,
  cancelPublicAiOAuth,
  commitPublicAiOAuthConnection,
  getPublicAiOAuthStatus,
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
  removePublicAiConnection,
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
  PublicAiOAuthTransaction,
  PublicAiProviderDescriptor,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

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
  // activeProviderLabel supplies the shared collapsed-menu pill label.
  // activeProviderLabel 为共享收起菜单胶囊提供标签。
  const activeProviderLabel = activeProviderId
    ? (() => {
      // provider resolves the exact registered descriptor selected by the left menu.
      // provider 解析左侧菜单选中的精确注册描述。
      const provider = directoryProviders.find((candidate) => candidate.id === activeProviderId)
      return provider
        ? localizedProviderName(provider.id, provider.display_name, t)
        : activeProviderId
    })()
    : t("settings.credentials.selectProvider")

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
    <AdaptiveToolPanel
      className="workspace-tool-panel-page workspace-skills-settings credential-management-layout"
      contentClassName="workspace-skills-content credential-management-content"
      contentShellClassName="credential-management-content-shell"
      expanded={menuExpanded}
      minContentWidth={336}
      panel={(
        <WorkspaceToolPanelMenu
          activeLabel={activeProviderLabel}
           ariaLabel={t("settings.credentials.categoryAria")}
           className="credential-management-menu"
           collapseLabel={t("settings.credentials.collapse")}
           expanded={menuExpanded}
           expandLabel={t("settings.credentials.expand")}
           heading={t("settings.credentials.categoryAria")}
          onExpandedChange={setMenuExpanded}
        >
          {directoryProviders.map((provider) => (
            <button
              className={`workspace-tool-panel-menu-row ${activeProviderId === provider.id ? "active" : ""}`}
              key={provider.id}
              onClick={() => setActiveProviderId(provider.id)}
              type="button"
            >
              <ProviderIcon category={provider.category} className="size-5" providerId={provider.id} />
              <span>{localizedProviderName(provider.id, provider.display_name, t)}</span>
              <small>{snapshot?.connections.filter((connection) => connection.provider_id === provider.id).length ?? 0}</small>
            </button>
          ))}
        </WorkspaceToolPanelMenu>
      )}
      panelWidth={176}
      side="left"
    >
      <div className="settings-direct-content credential-management-surface">
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
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        {!snapshot ? (
          <article className="settings-card">
            <strong>{t("settings.credentials.connecting")}</strong>
            <p>{t("settings.credentials.connectingDescription")}</p>
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
        ) : activeProviderId ? (
          <div className="flex min-h-48 w-full flex-col items-center justify-center gap-2 text-center text-muted-foreground">
            <strong>{t("settings.credentials.emptyProvider")}</strong>
            <p>{t("settings.credentials.emptyProviderDescription")}</p>
          </div>
        ) : (
          <div className="flex min-h-48 w-full flex-col items-center justify-center gap-2 text-center text-muted-foreground">
            <strong>{t("settings.credentials.emptyAll")}</strong>
            <p>{t("settings.credentials.emptyAllDescription")}</p>
          </div>
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
      </div>
    </AdaptiveToolPanel>
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
              label={t("settings.credentials.refreshUsage")}
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
            label={t("settings.credentials.syncModels")}
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
               ? t("settings.credentials.currentDefault")
               : t("settings.credentials.setDefault")}
          onClick={props.onSetDefault}
        >
          <Star
            className={props.isDefault
              ? "size-4 fill-yellow-400 text-yellow-500"
              : "size-4"}
          />
        </CredentialIconAction>
        <CredentialIconAction label={t("settings.credentials.edit")} onClick={props.onEdit}>
          <KeyRound className="size-4" />
        </CredentialIconAction>
        <CredentialIconAction
          disabled={props.busy}
          label={t("settings.credentials.remove")}
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
      setError(t("settings.credentials.deleteDefaultMissingReplacement"))
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
          <DialogTitle>{t("settings.credentials.deleteTitle")}</DialogTitle>
          <DialogDescription>
            {t("settings.credentials.deleteDescription").replace("{name}", props.connection?.display_name ?? "")}
          </DialogDescription>
        </DialogHeader>
        {props.isProviderDefault && replacements.length > 0 ? (
          <div className="settings-field">
            <span>{t("settings.credentials.replacementDefault")}</span>
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
            {t("settings.credentials.cancel")}
          </button>
          <button className="skill-action-button secondary danger" disabled={saving} onClick={() => void remove()} type="button">
            {saving ? t("settings.credentials.deleting") : t("settings.credentials.confirmDelete")}
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
  const { t } = useI18n()
  if (props.credentialProfile?.account_usage_query === "unsupported") {
    return <span>{t("settings.credentials.usageUnsupported")}</span>
  }
  if (!props.usage) {
    return <span>{t("settings.credentials.usageNotQueried")}</span>
  }
  if (props.usage.availability === "temporarily_unavailable") {
    return <span>{props.usage.refresh_message ?? t("settings.credentials.usageUnavailable")}</span>
  }
  if (!Array.isArray(props.usage.quota_windows)) {
    return <span className="text-destructive">{t("settings.credentials.usageInvalid")}</span>
  }
  return (
    <div className="settings-modal-list">
      <span>
        {props.usage.plan_name ? `${props.usage.plan_name} · ` : ""}
        {props.usage.remaining_amount !== undefined
          ? t("settings.credentials.remaining").replace("{value}", `${props.usage.remaining_amount}${props.usage.currency ? ` ${props.usage.currency}` : ""}`)
          : props.usage.quota_windows.length === 0 ? t("settings.credentials.noQuotaWindows") : t("settings.credentials.quotaWindow")}
        {props.usage.used_amount !== undefined
          ? ` · ${t("settings.credentials.used").replace("{value}", `${props.usage.used_amount}${props.usage.currency ? ` ${props.usage.currency}` : ""}`)}`
          : ""}
      </span>
      {props.usage.quota_windows.map((window) => (
        <span key={window.id}>
          {window.bucket_name ? `${window.bucket_name} · ` : ""}
          {window.display_name}{t("global.labelSeparator")}
          {!window.available
            ? t("settings.credentials.notIncluded")
            : window.unlimited
              ? t("settings.credentials.unlimited")
              : window.total !== undefined && window.total !== "0"
                ? `${window.remaining ?? t("settings.credentials.unknown")} / ${window.total} ${window.unit}${window.remaining_percent !== undefined ? ` · ${window.remaining_percent}%` : ""}`
                : window.remaining_percent !== undefined
                  ? t("settings.credentials.remaining").replace("{value}", `${window.remaining_percent}%`)
                  : `${window.remaining ?? t("settings.credentials.unknown")} / ${window.total ?? t("settings.credentials.unknown")} ${window.unit}`}
        </span>
      ))}
    </div>
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
      return
    }
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
      setError(t("settings.credentials.connectionIncomplete"))
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
              ? isProviderToolEntry ? t("settings.credentials.editToolTitle") : t("settings.credentials.editModelTitle")
              : isProviderToolEntry ? t("settings.credentials.addToolTitle") : t("settings.credentials.addModelTitle")}
          </DialogTitle>
          <DialogDescription>
            {isProviderToolEntry
              ? t("settings.credentials.toolDescription")
              : t("settings.credentials.modelDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
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
        </div>
        {oauthTransaction ? (
          <div className="settings-muted-line">
            <span>
              {t("settings.credentials.authorizationStatus")}{t("global.labelSeparator")}{oauthPhaseDisplayName(oauthTransaction.phase, t)}
              {oauthTransaction.message ? ` · ${oauthTransaction.message}` : ""}
            </span>
            {oauthUserCode ? <strong>{t("settings.credentials.authorizationCode")}{t("global.labelSeparator")}{oauthUserCode}</strong> : null}
          </div>
        ) : null}
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        <DialogFooter>
          <button
            className="skill-action-button secondary"
            onClick={() => pendingAuthorization ? void cancelAuthorization() : props.onOpenChange(false)}
            type="button"
          >
            {pendingAuthorization ? t("settings.credentials.cancelAuthorization") : t("settings.credentials.cancel")}
          </button>
          <button className="skill-action-button" disabled={saving || pendingAuthorization !== null} onClick={() => void save()} type="button">
            {pendingAuthorization
              ? t("settings.credentials.waitBrowser")
              : saving
                ? t("settings.credentials.saveSyncing")
                : credentialMode === "auth"
                  ? props.connection
                    ? t("settings.credentials.reauthorizeAndSave")
                    : t("settings.credentials.openBrowserAuthorize")
                  : t("settings.credentials.saveSync")}
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

// errorMessage normalizes unknown service failures for inline display.
// errorMessage 归一化未知服务失败以供行内展示。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
