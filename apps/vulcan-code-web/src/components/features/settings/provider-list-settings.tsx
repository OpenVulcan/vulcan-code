import * as React from "react"

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PublicAiModelOverridesDialog } from "@/components/features/settings/public-ai-model-overrides-dialog"
import {
  addPublicAiConnection,
  cancelPublicAiOAuth,
  commitPublicAiOAuthConnection,
  getPublicAiOAuthStatus,
  loadPublicAiSettings,
  queryPublicAiConnectionUsage,
  removePublicAiConnection,
  savePublicAiDefaultConnection,
  selectPublicAiModel,
  startPublicAiOAuth,
  syncPublicAiConnection,
  updatePublicAiConnection,
} from "@/service/settings-actions-service"
import type {
  CredentialMode,
  PublicAiAccountUsage,
  PublicAiConnection,
  PublicAiCredentialProfile,
  PublicAiModel,
  PublicAiOAuthTransaction,
  PublicAiProviderDescriptor,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

// ProvidersArea renders credential-scoped public AI connections and memory-only model catalogs.
// ProvidersArea 渲染凭据作用域公共 AI 连接与仅内存模型目录。
export function ProvidersArea(props: {
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
  const [editingModel, setEditingModel] = React.useState<{
    connection: PublicAiConnection
    model: PublicAiModel
  } | null>(null)
  const [deletingConnection, setDeletingConnection] = React.useState<PublicAiConnection | null>(null)
  const [usageByConnection, setUsageByConnection] = React.useState<Record<string, PublicAiAccountUsage>>({})

  // refresh loads the authoritative persisted graph and current process-local catalogs.
  // refresh 加载权威持久化连接图与当前进程内目录。
  const refresh = React.useCallback(async () => {
    try {
      setError(null)
      setSnapshot(await loadPublicAiSettings())
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

  return (
    <div className="settings-direct-content">
      {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
      {!snapshot ? (
        <article className="settings-card">
          <strong>正在连接公共 AI 运行时</strong>
          <p>启动后会从各凭据实时获取模型，不读取本地模型缓存。</p>
        </article>
      ) : snapshot.connections.length === 0 ? (
        <article className="settings-card">
          <strong>尚未添加模型账号</strong>
          <p>点击“添加 Provider”，选择供应商、服务入口、凭据方式与协议。</p>
        </article>
      ) : (
        <div className="card-grid">
          {snapshot.connections.map((connection) => {
            const provider = providerForConnection(snapshot, connection)
            const entry = entryForConnection(provider, connection)
            const credentialProfile = credentialProfileForConnection(entry, connection)
            const catalog = snapshot.catalogs[connection.id]
            const usage = usageByConnection[connection.id]
            const catalogError = snapshot.catalogErrors[connection.id]
            const isDefault = snapshot.providerDefaults[connection.provider_id] === connection.id
            return (
              <article className="data-card" key={connection.id}>
                <div className="settings-card-actions">
                  <span className="status-pill ready">{catalog ? "目录可用" : "目录不可用"}</span>
                  {isDefault ? <span className="status-pill pending">默认账号</span> : null}
                </div>
                <strong>{provider?.display_name ?? connection.provider_id} · {connection.display_name}</strong>
                <span>
                  {entry?.display_name ?? connection.service_entry_id}
                  {" · "}
                  {credentialProfile?.display_name ?? connection.credential_profile_id}
                </span>
                <span>{protocolDisplayName(connection.selected_protocol)}</span>
                <span>{catalog ? `${catalog.models.length} 个实时模型 · 第 ${catalog.generation} 次同步` : "尚未取得模型目录"}</span>
                {catalogError ? <span className="settings-error-text">{catalogError}</span> : null}
                <AccountUsageLine credentialProfile={credentialProfile} usage={usage} />
                {catalog && catalog.models.length > 0 ? (
                  <div className="settings-modal-list">
                    {catalog.models.map((model) => (
                      <div className="settings-picker-row" key={model.id}>
                        <button
                          className="skill-action-button secondary"
                          onClick={() => void runConnectionAction(
                            connection.id,
                            () => selectPublicAiModel({
                              providerId: connection.provider_id,
                              modelId: model.id,
                              connectionId: connection.id,
                            }),
                          )}
                          type="button"
                        >
                          <strong>{model.display_name}</strong>
                          <span>
                            {snapshot.active?.connection_id === connection.id && snapshot.active.model_id === model.id
                              ? "当前模型"
                              : model.id}
                          </span>
                        </button>
                        <button
                          className="skill-action-button secondary"
                          onClick={() => setEditingModel({ connection, model })}
                          type="button"
                        >
                          附加条件
                        </button>
                      </div>
                    ))}
                  </div>
                ) : null}
                <div className="settings-card-actions">
                  <button
                    className="skill-action-button secondary"
                    disabled={busyConnectionId === connection.id}
                    onClick={() => void runConnectionAction(
                      connection.id,
                      () => syncPublicAiConnection(connection.id),
                    )}
                    type="button"
                  >
                    {busyConnectionId === connection.id ? "处理中…" : "同步模型"}
                  </button>
                  {credentialProfile?.account_usage_query !== "unsupported" ? (
                    <button
                      className="skill-action-button secondary"
                      disabled={busyConnectionId === connection.id}
                      onClick={() => void refreshUsage(connection.id)}
                      type="button"
                    >
                      查询用量
                    </button>
                  ) : null}
                  <button
                    className="skill-action-button secondary"
                    onClick={() => setEditingConnection(connection)}
                    type="button"
                  >
                    编辑
                  </button>
                  {!isDefault ? (
                    <button
                      className="skill-action-button secondary"
                      disabled={busyConnectionId === connection.id}
                      onClick={() => void runConnectionAction(
                        connection.id,
                        () => savePublicAiDefaultConnection({
                          providerId: connection.provider_id,
                          connectionId: connection.id,
                        }),
                      )}
                      type="button"
                    >
                      设为默认
                    </button>
                  ) : null}
                  <button
                    className="skill-action-button secondary danger"
                    disabled={busyConnectionId === connection.id}
                    onClick={() => setDeletingConnection(connection)}
                    type="button"
                  >
                    删除
                  </button>
                </div>
              </article>
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
      <PublicAiModelOverridesDialog
        connection={editingModel?.connection ?? null}
        model={editingModel?.model ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setEditingModel(null)
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setEditingModel(null)
        }}
        open={editingModel !== null}
        overrides={snapshot?.modelOverrideRecords.find((record) => (
          record.connectionId === editingModel?.connection.id
          && record.modelId === editingModel?.model.id
        ))?.overrides}
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
  return (
    <div className="settings-modal-list">
      <span>
        {props.usage.plan_name ? `${props.usage.plan_name} · ` : ""}
        {props.usage.remaining_amount !== undefined
          ? `剩余 ${props.usage.remaining_amount}${props.usage.currency ? ` ${props.usage.currency}` : ""}`
          : props.usage.quota_windows.length === 0 ? "供应商未返回剩余额度" : "套餐额度窗口"}
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
    if (!provider || !entry || !credentialProfile || !displayName.trim() || !protocolId || (requiresToken && !secret)) {
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
        })
        : await addPublicAiConnection({
          providerId: provider.id,
          serviceEntryId: entry.id,
          credentialProfileId: credentialProfile.id,
          credentialMode,
          displayName,
          selectedProtocol: protocolId,
          secret,
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
    oauthClientId,
    oauthClientSecret,
    oauthProjectId,
    props,
    protocolId,
    provider,
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
          <DialogTitle>{props.connection ? "编辑模型账号" : "添加模型账号"}</DialogTitle>
          <DialogDescription>协议绑定在当前账号上，模型调用期间不会切换协议或账号。</DialogDescription>
        </DialogHeader>
        <div className="settings-form-grid">
          <label className="settings-form-field">
            <span>供应商</span>
            <select
              className="settings-input"
              disabled={props.connection !== null}
              onChange={(event) => setProviderId(event.target.value)}
              value={provider?.id ?? ""}
            >
              {props.providers.map((item) => <option key={item.id} value={item.id}>{item.display_name}</option>)}
            </select>
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
          {availableProtocols.length > 1 ? (
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
