import * as React from "react"
import { Boxes, KeyRound, Layers3, RefreshCw, Star, Trash2 } from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { CredentialUsageOverview } from "@/components/public-ai/credential-usage-overview"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
  PublicAiConnectionCatalog,
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
  const [usageByConnection, setUsageByConnection] = React.useState<Record<string, PublicAiAccountUsage>>({})
  const [activeProviderId, setActiveProviderId] = React.useState<string>("")
  const [menuExpanded, setMenuExpanded] = React.useState(true)

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

  // credentialProviders contains only providers that own at least one configured credential.
  // credentialProviders 仅包含至少拥有一个已配置凭据的供应商。
  const credentialProviders = snapshot?.providers.filter((provider) => (
    snapshot.connections.some((connection) => connection.provider_id === provider.id)
  )) ?? []
  // visibleConnections applies the exact left-menu provider filter.
  // visibleConnections 应用精确的左侧菜单供应商筛选。
  const visibleConnections = snapshot?.connections.filter((connection) => (
    !activeProviderId || connection.provider_id === activeProviderId
  )) ?? []
  // activeProviderLabel supplies the shared collapsed-menu pill label.
  // activeProviderLabel 为共享收起菜单胶囊提供标签。
  const activeProviderLabel = activeProviderId
    ? credentialProviders.find((provider) => provider.id === activeProviderId)?.display_name ?? activeProviderId
    : "全部凭据"

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
          {credentialProviders.map((provider) => (
            <button
              className={`workspace-tool-panel-menu-row ${activeProviderId === provider.id ? "active" : ""}`}
              key={provider.id}
              onClick={() => setActiveProviderId(provider.id)}
              type="button"
            >
              <ProviderIcon className="size-5" providerId={provider.id} />
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
            <select
              className="settings-input"
              onChange={(event) => void saveUsageRefreshInterval(Number(event.target.value))}
              value={snapshot.usageRefreshIntervalSeconds}
            >
              <option value={60}>每 1 分钟</option>
              <option value={300}>每 5 分钟</option>
              <option value={900}>每 15 分钟</option>
              <option value={1800}>每 30 分钟</option>
              <option value={3600}>每 1 小时</option>
            </select>
            <small>仅刷新声明支持用量查询且已具备必要凭据的账号，缓存只保存在内存中。</small>
          </div>
        ) : null}
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        {!snapshot ? (
          <article className="settings-card">
            <strong>正在连接公共 AI 运行时</strong>
            <p>启动后会从各凭据实时获取模型，不读取本地模型缓存。</p>
          </article>
        ) : snapshot.connections.length === 0 ? (
          <article className="settings-card">
            <strong>尚未添加模型账号</strong>
            <p>点击“添加凭据”，选择供应商、服务入口、凭据方式与协议。</p>
          </article>
        ) : (
          <div className="credential-management-list">
            {visibleConnections.map((connection) => {
              const provider = providerForConnection(snapshot, connection)
              const entry = entryForConnection(provider, connection)
              const credentialProfile = credentialProfileForConnection(entry, connection)
              const catalog = snapshot.catalogs[connection.id]
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
                  catalog={catalog}
                  catalogError={catalogError}
                  connection={connection}
                  credentialProfile={credentialProfile}
                  entry={entry}
                  isDefault={isDefault}
                  key={connection.id}
                  onDelete={() => setDeletingConnection(connection)}
                  onEdit={() => setEditingConnection(connection)}
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
        )}
        <PublicAiConnectionDialog
        connection={null}
        onOpenChange={props.onProviderPickerOpenChange}
        onSaved={(next) => {
          setSnapshot(next)
          props.onProviderPickerOpenChange(false)
        }}
        open={props.pickerOpen}
        providers={snapshot?.providers ?? []}
      />
      <PublicAiConnectionDialog
        connection={editingConnection}
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
  // catalog is the current memory-only model catalog.
  // catalog 是当前仅内存模型目录。
  catalog?: PublicAiConnectionCatalog
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
  const usageSupported = props.credentialProfile?.account_usage_query !== "unsupported"
    && (props.connection.provider_id !== "openrouter" || props.connection.has_management_key)
  const providerName = props.provider?.display_name ?? props.connection.provider_id
  const authorizationName = props.credentialProfile?.display_name
    ?? props.connection.credential_profile_id
  // connectionAvailable treats provider-tool entries as immediately usable because they have no model catalog to sync.
  // connectionAvailable 将供应商工具入口视为立即可用，因为它们没有需要同步的模型目录。
  const connectionAvailable = props.entry?.kind === "provider_tools" || Boolean(props.catalog)
  return (
    <Card
      className="max-h-[18rem] w-full max-w-[21rem] gap-0 border border-border ring-0"
      size="sm"
    >
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <ProviderIcon
            className="size-8 shrink-0"
            providerId={props.connection.provider_id}
            serviceEntryId={props.connection.service_entry_id}
          />
          <div className="min-w-0 flex-1">
            <div className="mb-1 flex flex-wrap items-center gap-1.5">
              <span className="truncate text-xs text-muted-foreground">
                {props.entry?.display_name ?? providerName}
              </span>
              <Badge variant="outline">{connectionAvailable ? "可用" : "待同步"}</Badge>
            </div>
            <CardTitle className="truncate">{props.connection.display_name}</CardTitle>
            <CardDescription className="truncate text-xs">
              {authorizationName}
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

      <CardContent className="mt-3 flex flex-1 flex-col gap-3">
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

      <CardFooter className="mt-auto flex-wrap justify-end gap-2">
        {props.entry?.kind !== "provider_tools" ? (
          <CredentialIconAction
            disabled={props.busy}
            label="同步支持模型"
            onClick={props.onSync}
          >
            <Boxes className="size-4" />
          </CredentialIconAction>
        ) : null}
        {!props.isDefault ? (
          <CredentialIconAction
            disabled={props.busy}
            label="设为默认账号"
            onClick={props.onSetDefault}
          >
            <Star className="size-4" />
          </CredentialIconAction>
        ) : (
          <Badge variant="secondary">默认</Badge>
        )}
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
          <label className="settings-field">
            <span>替代默认账号</span>
            <select onChange={(event) => setReplacementId(event.target.value)} value={replacementId}>
              {replacements.map((connection) => (
                <option key={connection.id} value={connection.id}>{connection.display_name}</option>
              ))}
            </select>
          </label>
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
  const [providerId, setProviderId] = React.useState("")
  const [entryId, setEntryId] = React.useState("")
  const [credentialProfileId, setCredentialProfileId] = React.useState("")
  const [credentialMode, setCredentialMode] = React.useState<CredentialMode>("token")
  const [displayName, setDisplayName] = React.useState("")
  const [protocolId, setProtocolId] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [managementKey, setManagementKey] = React.useState("")
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
    selectedProtocol: string
    connectionId?: string
  } | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)

  const provider = props.providers.find((item) => item.id === providerId) ?? props.providers[0]
  const entry = provider?.service_entries.find((item) => item.id === entryId) ?? provider?.service_entries[0]
  const credentialProfile = entry?.credential_profiles.find((item) => item.id === credentialProfileId)
    ?? entry?.credential_profiles[0]
  const availableProtocols = entry?.protocol_routes.filter((route) => route.verified) ?? []
  const isProviderToolEntry = entry?.kind === "provider_tools"

  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const existingProvider = props.connection
      ? props.providers.find((item) => item.id === props.connection?.provider_id)
      : props.providers[0]
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
    setProtocolId(
      props.connection?.selected_protocol
      ?? existingEntry?.protocol_routes.find((route) => route.verified)?.protocol_profile_id
      ?? "",
    )
    setSecret("")
    setManagementKey("")
    setRemoveManagementKey(false)
    setOauthClientId("")
    setOauthClientSecret("")
    setOauthProjectId("")
    setOauthTransaction(null)
    setOauthUserCode(null)
    setPendingAuthorization(null)
    setError(null)
  }, [props.connection, props.open, props.providers])

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
    if (!availableProtocols.some((route) => route.protocol_profile_id === protocolId)) {
      setProtocolId(availableProtocols[0]?.protocol_profile_id ?? "")
    }
  }, [availableProtocols, credentialMode, credentialProfileId, entry, protocolId])

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

  // save validates the visible form and delegates secrets to the operating-system store.
  // save 校验可见表单并把秘密委托给操作系统凭据库。
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
      || (!isProviderToolEntry && !protocolId)
      || (requiresToken && !secret)
    ) {
      setError("请完整填写连接信息。")
      return
    }
    setSaving(true)
    setError(null)
    try {
      if (credentialMode === "auth") {
        if (!credentialProfile.oauth_provider) {
          throw new Error("The selected credential profile does not declare an OAuth flow.")
        }
        if (
          (credentialProfile.oauth_provider === "google_code_assist" || credentialProfile.oauth_provider === "google_gemini")
          && !oauthClientId.trim()
        ) {
          throw new Error("Google OAuth requires a desktop OAuth client id.")
        }
        if (credentialProfile.oauth_provider === "google_gemini" && !oauthProjectId.trim()) {
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
          selectedProtocol: protocolId,
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
          selectedProtocol: protocolId,
          ...(secret ? { secret } : {}),
          ...(managementKey ? { managementKey } : {}),
          ...(removeManagementKey ? { removeManagementKey: true } : {}),
        })
        : await addPublicAiConnection({
          providerId: provider.id,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          credentialMode,
          displayName,
          selectedProtocol: protocolId,
          secret,
          ...(managementKey ? { managementKey } : {}),
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
    protocolId,
    provider,
    managementKey,
    removeManagementKey,
    secret,
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
      <DialogContent className="settings-dialog">
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
              <ProviderIcon providerId={provider?.id} />
              <select
                className="settings-input"
                disabled={props.connection !== null}
                onChange={(event) => setProviderId(event.target.value)}
                value={provider?.id ?? ""}
              >
                {props.providers.map((item) => <option key={item.id} value={item.id}>{item.display_name}</option>)}
              </select>
            </span>
          </label>
          <label className="settings-form-field">
            <span>服务入口</span>
            {provider?.id === "minimax" && credentialMode === "token" ? (
              <strong>保存时自动识别区域</strong>
            ) : (
              <select className="settings-input" onChange={(event) => setEntryId(event.target.value)} value={entry?.id ?? ""}>
                {(provider?.service_entries ?? []).map((item) => <option key={item.id} value={item.id}>{item.display_name}</option>)}
              </select>
            )}
          </label>
          <label className="settings-form-field">
            <span>凭据类型</span>
            <select
              className="settings-input"
              onChange={(event) => {
                const nextProfile = entry?.credential_profiles.find((item) => item.id === event.target.value)
                setCredentialProfileId(event.target.value)
                setCredentialMode(nextProfile?.credential_mode ?? "token")
              }}
              value={credentialProfile?.id ?? ""}
            >
              {(entry?.credential_profiles ?? []).map((profile) => (
                <option key={profile.id} value={profile.id}>{profile.display_name}</option>
              ))}
            </select>
          </label>
          <label className="settings-form-field">
            <span>账号显示名</span>
            <input className="settings-input" onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
          </label>
          {isProviderToolEntry ? null : availableProtocols.length > 1 ? (
            <label className="settings-form-field">
              <span>调用协议</span>
              <select className="settings-input" onChange={(event) => setProtocolId(event.target.value)} value={protocolId}>
                {availableProtocols.map((route) => (
                  <option key={route.protocol_profile_id} value={route.protocol_profile_id}>
                    {protocolDisplayName(route.protocol_profile_id)}
                  </option>
                ))}
              </select>
            </label>
          ) : (
            <div className="settings-form-field">
              <span>调用协议</span>
              <strong>{protocolDisplayName(protocolId)}</strong>
            </div>
          )}
          {credentialMode === "token" ? (
            <label className="settings-form-field">
              <span>{props.connection ? "替换 Token（留空则不变）" : "API Token"}</span>
              <input
                autoComplete="off"
                className="settings-input"
                onChange={(event) => setSecret(event.target.value)}
                type="password"
                value={secret}
              />
            </label>
          ) : null}
          {provider?.id === "openrouter" ? (
            <div className="settings-form-field">
              <span>管理密钥（可选）</span>
              <input
                autoComplete="off"
                className="settings-input"
                onChange={(event) => setManagementKey(event.target.value)}
                placeholder={props.connection?.has_management_key ? "留空保持现有管理密钥" : ""}
                type="password"
                value={managementKey}
              />
              <small>仅用于查询 OpenRouter 账号余额，不用于模型调用。</small>
              {props.connection?.has_management_key ? (
                <label className="settings-inline-checkbox">
                  <input
                    checked={removeManagementKey}
                    onChange={(event) => setRemoveManagementKey(event.target.checked)}
                    type="checkbox"
                  />
                  <span>移除现有管理密钥</span>
                </label>
              ) : null}
            </div>
          ) : null}
          {credentialMode === "auth"
            && (credentialProfile?.oauth_provider === "google_code_assist" || credentialProfile?.oauth_provider === "google_gemini") ? (
            <>
              <label className="settings-form-field">
                <span>Google OAuth Client ID</span>
                <input className="settings-input" onChange={(event) => setOauthClientId(event.target.value)} value={oauthClientId} />
              </label>
              <label className="settings-form-field">
                <span>Google OAuth Client Secret（可选）</span>
                <input
                  autoComplete="off"
                  className="settings-input"
                  onChange={(event) => setOauthClientSecret(event.target.value)}
                  type="password"
                  value={oauthClientSecret}
                />
              </label>
              {credentialProfile.oauth_provider === "google_gemini" ? (
                <label className="settings-form-field">
                  <span>Google Cloud Project ID</span>
                  <input
                    className="settings-input"
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
    exchanging_token: "正在交换授权令牌",
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
