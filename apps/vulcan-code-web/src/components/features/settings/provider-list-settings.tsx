import * as React from "react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { usageCapabilityRequiresManagementKey } from "@/components/public-ai/credential-usage-capability"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useI18n } from "@/i18n"
import {
  addPublicAiConnection,
  cancelPublicAiOAuth,
  commitPublicAiOAuthConnection,
  getPublicAiOAuthStatus,
  removePublicAiConnection,
  startPublicAiOAuth,
  updatePublicAiConnection,
} from "@/service/settings-actions-service"
import type {
  CredentialMode,
  PublicAiConnection,
  PublicAiModelDirectoryKind,
  PublicAiOAuthTransaction,
  PublicAiProviderDescriptor,
  PublicAiSettingsSnapshot,
} from "@/types"
import { errorMessage } from "@/lib/display-format";

// credentialSupportsModelSynchronization identifies directories whose model membership can change remotely.
// credentialSupportsModelSynchronization 标识模型成员可由远端变化的目录。
export function credentialSupportsModelSynchronization(
  directoryKind: PublicAiModelDirectoryKind | undefined,
): boolean {
  return directoryKind === "dynamic"
}

// PublicAiDeleteConnectionDialog confirms destructive removal and requires an explicit replacement default.
// PublicAiDeleteConnectionDialog 确认破坏性移除，并要求显式选择替代默认账号。
export function PublicAiDeleteConnectionDialog(props: {
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
  // Service entry preselected when creating; ignored while editing or when the provider lacks it.
  // 创建时预选的服务入口；编辑时或供应商没有该入口时忽略。
  initialEntryId?: string
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
    const connectionKey = JSON.stringify([props.connection?.id ?? null, props.lockedProviderId, props.initialProviderId, props.initialEntryId ?? null])
    if (initializedConnectionKey.current === connectionKey) return
    initializedConnectionKey.current = connectionKey
    const existingProvider = props.connection
      ? props.providers.find((item) => item.id === props.connection?.provider_id)
      : props.providers.find((item) => item.id === props.lockedProviderId)
        ?? props.providers.find((item) => item.id === props.initialProviderId)
        ?? (props.lockedProviderId ? undefined : props.providers[0])
    const existingEntry = props.connection
      ? existingProvider?.service_entries.find((item) => item.id === props.connection?.service_entry_id)
      : existingProvider?.service_entries.find((item) => item.id === props.initialEntryId)
        ?? existingProvider?.service_entries[0]
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
  }, [props.connection, props.initialEntryId, props.initialProviderId, props.lockedProviderId, props.open, props.providers])

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
          {provider && provider.service_entries.length > 1 && <label className="settings-form-field">
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
          </label>}
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
