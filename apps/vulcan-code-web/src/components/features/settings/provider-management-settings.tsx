import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout"
import * as React from "react"
import { Trash2 } from "lucide-react"
import { PublicAiConnectionDialog } from "@/components/features/settings/provider-list-settings"
import { isFunctionalProvider } from "@/components/features/settings/provider-ordering"
import { CustomProviderModelsDialog } from "@/components/features/settings/custom-provider-models-dialog"
import { SystemProviderModelsDialog } from "@/components/features/settings/system-provider-models-dialog"
import { ProviderManifestStatusPanel } from "@/components/features/settings/provider-manifest-status"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { useI18n } from "@/i18n"
import { loadPublicAiSettings, removePublicAiCustomProvider, savePublicAiProviderPriority } from "@/service/settings-actions-service"
import type { PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"
import { errorMessage } from "@/lib/display-format"
import { CustomProviderDialog } from "./provider-management-settings/custom-provider-dialog"
import { CustomProviderTable, SystemProviderTable } from "./provider-management-settings/provider-tables"

// ProviderManagementArea renders the complete provider registry without credential details.
// ProviderManagementArea 渲染不含凭据详情的完整供应商注册表。
export function ProviderManagementArea() {
  // account changes invalidate the visible core-provider directory.
  // account 的变化使可见核心供应商目录失效。
  const account = React.useContext(OpenVulcanAccountContext)
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [editingProvider, setEditingProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [dialogOpen, setDialogOpen] = React.useState(false)
  const [modelsProvider, setModelsProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [modelsDialogOpen, setModelsDialogOpen] = React.useState(false)
  const [busyProviderId, setBusyProviderId] = React.useState<string | null>(null)
  const [providerToRemove, setProviderToRemove] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [isolatedProviderToRemove, setIsolatedProviderToRemove] = React.useState<string | null>(null)
  // credentialProviderId owns the exact provider locked into the add-credential dialog.
  // credentialProviderId 持有锁定到新增凭据弹窗的精确供应商。
  const [credentialProviderId, setCredentialProviderId] = React.useState("")
  const [error, setError] = React.useState<string | null>(null)
  // category selects one local provider directory without changing persisted provider state.
  // category 选择一个局部供应商目录，不改变持久化供应商状态。
  const [category, setCategory] = React.useState("system")
  // Full settings snapshots must be applied in the same order as provider mutations.
  // 完整设置快照必须按供应商变更顺序采用。
  const providerActionRunningRef = React.useRef(false)

  // refresh loads the effective system and custom provider registry.
  // refresh 加载有效的系统与自定义供应商注册表。
  const refresh = React.useCallback(async () => {
    setError(null)
    try {
      setSnapshot(await loadPublicAiSettings())
    } catch (reason) {
      setError(errorMessage(reason))
    }
  }, [])

  React.useEffect(() => {
    void refresh()
  }, [refresh, account?.snapshot?.connectionId, account?.snapshot?.connected])

  // openCreate starts a blank custom-provider editor.
  // openCreate 启动空白自定义供应商编辑器。
  const openCreate = React.useCallback(() => {
    setEditingProvider(null)
    setDialogOpen(true)
  }, [])

  // openEdit starts an editor from one exact custom descriptor.
  // openEdit 使用一个精确自定义描述启动编辑器。
  const openEdit = React.useCallback((provider: PublicAiProviderDescriptor) => {
    setEditingProvider(provider)
    setDialogOpen(true)
  }, [])

  // openModels opens the credential-independent model overlay manager.
  // openModels 打开与凭据无关的模型覆盖管理器。
  const openModels = React.useCallback((provider: PublicAiProviderDescriptor) => {
    setModelsProvider(provider)
    setModelsDialogOpen(true)
  }, [])

  // handleProviderSaved refreshes state and guides newly created providers into model setup.
  // handleProviderSaved 刷新状态，并引导新建供应商继续完成模型配置。
  const handleProviderSaved = React.useCallback((nextSnapshot: PublicAiSettingsSnapshot) => {
    const previousSnapshot = snapshot
    setSnapshot(nextSnapshot)
    if (!previousSnapshot) {
      return
    }
    const previousProviderIds = new Set(previousSnapshot.providers.map((provider) => provider.id))
    const createdProviders = nextSnapshot.providers.filter((provider) => (
      provider.kind === "custom" && !previousProviderIds.has(provider.id)
    ))
    if (createdProviders.length === 1) {
      setModelsProvider(createdProviders[0])
      setModelsDialogOpen(true)
    }
  }, [snapshot])

  // removeProvider accepts one exact descriptor, cascades confirmed cleanup, and refreshes the local snapshot.
  // removeProvider 接收一个精确描述，级联执行已确认清理，并刷新本地快照。
  const removeProvider = React.useCallback(async (provider: PublicAiProviderDescriptor) => {
    if (providerActionRunningRef.current) return
    providerActionRunningRef.current = true
    setBusyProviderId(provider.id)
    setError(null)
    try {
      setSnapshot(await removePublicAiCustomProvider(provider.id))
      setProviderToRemove(null)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      providerActionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  // removeIsolatedProvider deletes one exact failed standalone reference without fabricating a descriptor.
  // removeIsolatedProvider 不伪造描述符，直接删除一项精确失败的独立引用。
  const removeIsolatedProvider = React.useCallback(async (providerId: string) => {
    if (providerActionRunningRef.current) return
    providerActionRunningRef.current = true
    setBusyProviderId(providerId)
    setError(null)
    try {
      setSnapshot(await removePublicAiCustomProvider(providerId))
      setIsolatedProviderToRemove(null)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      providerActionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  // saveProviderPriority persists one provider ordering value and refreshes the authoritative snapshot.
  // saveProviderPriority 持久化一个供应商排序值并刷新权威快照。
  const saveProviderPriority = React.useCallback(async (providerId: string, priority: number) => {
    if (providerActionRunningRef.current) return
    providerActionRunningRef.current = true
    setBusyProviderId(providerId)
    setError(null)
    try {
      setSnapshot(await savePublicAiProviderPriority(providerId, priority))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      providerActionRunningRef.current = false
      setBusyProviderId(null)
    }
  }, [])

  // systemProviders contains immutable model providers and excludes tool-only functional providers.
  // systemProviders 包含不可变模型供应商，并排除仅工具型功能供应商。
  const systemProviders = snapshot?.providers.filter((provider) => (
    provider.kind === "system" && !isFunctionalProvider(provider)
  )) ?? []
  // functionalProviders contains immutable providers whose service entries expose provider tools only.
  // functionalProviders 包含服务入口仅公开供应商工具的不可变供应商。
  const functionalProviders = snapshot?.providers.filter(isFunctionalProvider) ?? []
  // customProviders contains the user-managed provider definitions.
  // customProviders 包含用户管理的供应商定义。
  const customProviders = snapshot?.providers.filter((provider) => provider.kind === "custom") ?? []
  // customProviderManifestFailures are backend-bounded startup failures; the browser never
  // reparses invalid source bytes or guesses a fallback provider.
  // customProviderManifestFailures 是后端有界的启动失败；浏览器绝不重新解析
  // 无效源字节，也不猜测回退供应商。
  const customProviderManifestFailures = snapshot?.customProviderManifestFailures ?? []
  // customProviderManifestFailureTotalCount keeps the warning truthful when details are truncated.
  // customProviderManifestFailureTotalCount 在详情被截断时保持警告数量真实。
  const customProviderManifestFailureTotalCount = Math.max(
    snapshot?.customProviderManifestFailureTotalCount ?? 0,
    customProviderManifestFailures.length,
  )

  // providerToRemoveConnectionCount counts credentials that the confirmation may delete with the provider.
  // providerToRemoveConnectionCount 统计确认后可能与供应商一并删除的凭据数量。
  const providerToRemoveConnectionCount = snapshot && providerToRemove
    ? snapshot.connections.filter((connection) => connection.provider_id === providerToRemove.id).length
    : 0
  // providerToRemoveConnectionIds scopes account-level model settings to the provider being removed.
  // providerToRemoveConnectionIds 将账号级模型设置限定到当前待移除供应商。
  const providerToRemoveConnectionIds = new Set(
    snapshot && providerToRemove
      ? snapshot.connections
          .filter((connection) => connection.provider_id === providerToRemove.id)
          .map((connection) => connection.id)
      : [],
  )
  // providerToRemoveModelCustomizationCount counts model-owned settings disclosed by cascade cleanup.
  // providerToRemoveModelCustomizationCount 统计级联清理提示需要披露的模型专属设置。
  const providerToRemoveModelCustomizationCount = snapshot && providerToRemove
    ? [
        Object.keys(snapshot.customModels?.[providerToRemove.id] ?? {}).length,
        snapshot.customProviderModelDefaults?.[providerToRemove.id] ? 1 : 0,
        snapshot.customModelTemplateDecisionRecords?.filter((record) => record.providerId === providerToRemove.id).length ?? 0,
        snapshot.modelOverrideRecords.filter((record) => record.providerId === providerToRemove.id).length,
        snapshot.modelAccessRecords.filter((record) => (
          record.providerId === providerToRemove.id && record.enabled !== record.defaultEnabled
        )).length,
        snapshot.modelCapabilityOverrideRecords?.filter((record) => record.providerId === providerToRemove.id).length ?? 0,
        snapshot.customModelConnectionAvailabilityRecords?.filter((record) => (
          snapshot.connections.some((connection) => connection.id === record.connectionId && connection.provider_id === providerToRemove.id)
        )).length ?? 0,
        snapshot.connectionModelAccessPolicyRecords.filter((record) => (
          providerToRemoveConnectionIds.has(record.connectionId)
          && (!record.defaultEnabled || Object.keys(record.modelEnabledOverrides).length > 0)
        )).length,
        Object.values(snapshot.inferenceProfiles).reduce((count, profile) => (
          count
          + Object.values(profile.llm_routes ?? {}).filter((route) => route.provider_id === providerToRemove.id).length
          + Number(profile.embedding?.provider_id === providerToRemove.id)
          + Number(profile.rerank?.provider_id === providerToRemove.id)
        ), 0),
      ].reduce((total, count) => total + count, 0)
    : 0
  // providerHasModelCustomizations includes backend-owned settings that the removal warning cannot count locally.
  // providerHasModelCustomizations 包含移除提示无法在本地计数的后端专属设置。
  const providerHasModelCustomizations = Boolean(
    providerToRemove
    && snapshot?.providersWithModelCustomizations?.includes(providerToRemove.id),
  )
  // providerRemovalDescription combines every destructive consequence known by the settings snapshot.
  // providerRemovalDescription 组合设置快照已知的全部破坏性后果。
  const providerRemovalDescription = providerToRemove
    ? [
        t("settings.customProviders.removeDescription").replace("{name}", providerToRemove.display_name),
        providerHasModelCustomizations
          ? providerToRemoveModelCustomizationCount > 0
            ? t("settings.customProviders.removeModelsWarning").replace("{count}", String(providerToRemoveModelCustomizationCount))
            : t("settings.customProviders.removeModelsWarningGeneric")
          : null,
        providerToRemoveConnectionCount > 0
          ? t("settings.customProviders.removeCredentialsWarning").replace("{count}", String(providerToRemoveConnectionCount))
          : null,
        t("settings.customProviders.removeSessionsWarning"),
      ].filter((value): value is string => value !== null)
    : []

  return (
    <WorkspaceDirectoryLayout contentClassName="provider-management-surface space-y-6" navigationMode="tabs" navigation={<WorkspaceDirectoryMenu ariaLabel={t("settings.providers.managementTitle")} heading={t("settings.providers.managementTitle")}>
            {([
              ["system", t("settings.providers.system"), systemProviders.length],
              ["functional", t("settings.providers.functional"), functionalProviders.length],
              ["custom", t("settings.providers.custom"), customProviders.length],
              ["catalog", t("settings.providers.catalog"), null],
            ] as const).map(([value, label, count]) => (
              <button
                aria-current={category === value ? "page" : undefined}
                className={`workspace-tool-panel-menu-row ${category === value ? "active" : ""}`}
                key={value}
                onClick={() => setCategory(value)}
                type="button"
              >
                {/* Counts wait for the directory so a loading page never claims zero providers. / 数量等待目录读取完成，加载中不显示为零个供应商。 */}
                <span className="settings-directory-label">{label}</span>{count !== null && snapshot ? <small>{count}</small> : null}
              </button>
            ))}
          </WorkspaceDirectoryMenu>}>
      {error ? (
        <div className="rounded-xl border border-destructive/40 p-4 text-sm text-destructive">
          {error}
          {!snapshot ? <Button className="mt-2" onClick={() => void refresh()} type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button> : null}
        </div>
      ) : null}

      <div className="provider-management-categories">
        <section hidden={category !== "system"}>
          <SystemProviderTable
            busyProviderId={busyProviderId}
            connections={snapshot?.connections ?? []}
            loading={!snapshot}
            onAddCredential={setCredentialProviderId}
            onManageModels={openModels}
            onSavePriority={(providerId, priority) => void saveProviderPriority(providerId, priority)}
            priorities={snapshot?.providerPriorities ?? {}}
            providers={systemProviders}
            variant="system"
          />
        </section>
        <section hidden={category !== "functional"}>
          <SystemProviderTable
            busyProviderId={busyProviderId}
            connections={snapshot?.connections ?? []}
            loading={!snapshot}
            onAddCredential={setCredentialProviderId}
            onSavePriority={(providerId, priority) => void saveProviderPriority(providerId, priority)}
            priorities={snapshot?.providerPriorities ?? {}}
            providers={functionalProviders}
            variant="functional"
          />
        </section>
        <section className="space-y-4" hidden={category !== "custom"}>
          {customProviderManifestFailureTotalCount > 0 ? (
            <div className="rounded-xl border border-destructive/40 bg-destructive/5 p-4 text-sm">
              <p className="font-semibold text-destructive">
                {t("settings.providers.isolatedCount").replace("{count}", String(customProviderManifestFailureTotalCount))}
              </p>
              <p className="mt-1 text-muted-foreground">
                {t("settings.providers.isolatedDescription")}
              </p>
              <ul className="mt-2 space-y-2 text-xs text-destructive">
                {customProviderManifestFailures.map((failure, index) => (
                  <li
                    className="flex flex-wrap items-start justify-between gap-2 rounded-lg border border-destructive/20 p-2"
                    key={`${failure.providerId}:${index}`}
                  >
                    <span className="min-w-0 flex-1 break-all font-mono">
                      {failure.providerId}：{failure.message}
                    </span>
                    <Button
                      className="shrink-0"
                      disabled={busyProviderId !== null}
                      onClick={() => setIsolatedProviderToRemove(failure.providerId)}
                      size="sm"
                      type="button"
                      variant="destructive"
                    >
                      <Trash2 className="mr-1 size-3.5" />
                      {t("settings.providers.removeIsolated")}
                    </Button>
                  </li>
                ))}
              </ul>
              {snapshot?.customProviderManifestFailuresTruncated ? (
                <p className="mt-2 text-xs text-muted-foreground">{t("settings.providers.diagnosticsOmitted")}</p>
              ) : null}
            </div>
          ) : null}
          <CustomProviderTable
            busyProviderId={busyProviderId}
            connections={snapshot?.connections ?? []}
            customModels={snapshot?.customModels ?? {}}
            loading={!snapshot}
            onAddCredential={setCredentialProviderId}
            onCreate={openCreate}
            onEdit={openEdit}
            onManageModels={openModels}
            onRemove={setProviderToRemove}
            onSavePriority={(providerId, priority) => void saveProviderPriority(providerId, priority)}
            priorities={snapshot?.providerPriorities ?? {}}
            providers={customProviders}
          />
        </section>
        {category === "catalog" ? <ProviderManifestStatusPanel /> : null}
      </div>

      <CustomProviderDialog
        connections={snapshot?.connections ?? []}
        modelOverrideRecords={snapshot?.modelOverrideRecords ?? []}
        onOpenChange={setDialogOpen}
        onSaved={handleProviderSaved}
        open={dialogOpen}
        provider={editingProvider}
      />
      {modelsProvider?.kind === "custom" ? (
        <CustomProviderModelsDialog
          onOpenChange={setModelsDialogOpen}
          onSaved={setSnapshot}
          open={modelsDialogOpen}
          provider={modelsProvider}
          snapshot={snapshot}
        />
      ) : null}
      {modelsProvider?.kind === "system" && snapshot ? (
        <SystemProviderModelsDialog
          onOpenChange={setModelsDialogOpen}
          onSaved={setSnapshot}
          open={modelsDialogOpen}
          provider={modelsProvider}
          snapshot={snapshot}
        />
      ) : null}
      <PublicAiConnectionDialog
        connection={null}
        initialProviderId={credentialProviderId}
        lockedProviderId={credentialProviderId}
        onOpenChange={(open) => {
          if (!open) {
            setCredentialProviderId("")
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setCredentialProviderId("")
        }}
        open={credentialProviderId !== ""}
        providers={snapshot?.providers ?? []}
      />
      <AlertDialog onOpenChange={(open) => !open && !providerActionRunningRef.current && setProviderToRemove(null)} open={providerToRemove !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customProviders.removeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              <div className="space-y-2">
                {providerRemovalDescription.map((description) => <p key={description}>{description}</p>)}
              </div>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busyProviderId !== null}>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={providerToRemove
                ? busyProviderId === providerToRemove.id
                : true}
              onClick={(event) => {
                event.preventDefault()
                if (providerToRemove) {
                  void removeProvider(providerToRemove)
                }
              }}
              variant="destructive"
            >
              {t("settings.customProviders.removeTitle")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog
        onOpenChange={(open) => !open && !providerActionRunningRef.current && setIsolatedProviderToRemove(null)}
        open={isolatedProviderToRemove !== null}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.providers.removeIsolatedTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              <span className="block space-y-2">
                <span className="block">
                  {t("settings.providers.removeIsolatedDescription")}
                </span>
                <span className="block break-all font-mono text-foreground">
                  {isolatedProviderToRemove}
                </span>
              </span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busyProviderId !== null}>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={isolatedProviderToRemove
                ? busyProviderId === isolatedProviderToRemove
                : true}
              onClick={(event) => {
                event.preventDefault()
                if (isolatedProviderToRemove) {
                  void removeIsolatedProvider(isolatedProviderToRemove)
                }
              }}
              variant="destructive"
            >
              {t("settings.providers.permanentlyDelete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </WorkspaceDirectoryLayout>
  )
}

