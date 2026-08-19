import * as React from "react"
import {
  CircleAlert,
  CircleCheckBig,
  LoaderCircle,
  ListTree,
  LockKeyhole,
  Pencil,
  Plus,
  ServerCog,
  Trash2,
} from "lucide-react"

import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { CustomProviderModelsDialog } from "@/components/features/settings/custom-provider-models-dialog"
import { persistAndApplyCustomProviderSnapshot } from "@/components/features/settings/custom-provider-save-state"
import { ProviderManifestStatusPanel } from "@/components/features/settings/provider-manifest-status"
import { localizedProviderDescription } from "@/components/features/settings/provider-presentation"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import type { TranslationKey } from "@/i18n-messages"
import {
  CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE,
  codexResponsesFeaturePoliciesFromEntry,
  customProtocolRouteDraftValue,
  customProtocolRouteSelectValue,
  customProtocolProfileId,
  emptyProtocolRouteDraft,
  protocolCollectionChanged,
  protocolCollectionFromEntry,
  protocolRoutesFromCollection,
  protocolSetChange,
  sortProtocolsByAutoPriority,
} from "@/components/features/settings/custom-provider-protocol-drafts"
import type {
  CustomProtocolRouteDraft,
  CustomProviderProtocolCollection,
} from "@/components/features/settings/custom-provider-protocol-drafts"
import {
  loadPublicAiSettings,
  probePublicAiCustomProvider,
  removePublicAiCustomProvider,
  savePublicAiCustomProvider,
} from "@/service/settings-actions-service"
import type {
  PublicAiCustomProviderModelDiscoveryMode,
  PublicAiCustomProviderProbeResult,
  PublicAiCustomProviderProtocol,
  PublicAiCustomProviderProtocolRoute,
  PublicAiCustomProviderSaveRequest,
  PublicAiResponsesRemoteCompactionPolicy,
  PublicAiResponsesWebsocketPolicy,
  PublicAiConnection,
  PublicAiModelCapabilityEvidence,
  PublicAiModelOverrideRecord,
  PublicAiProviderDescriptor,
  PublicAiProtocolRoute,
  PublicAiServiceEntry,
  PublicAiSettingsSnapshot,
} from "@/types"

// ProviderManagementArea renders the complete provider registry without credential details.
// ProviderManagementArea 渲染不含凭据详情的完整供应商注册表。
export function ProviderManagementArea() {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [editingProvider, setEditingProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [dialogOpen, setDialogOpen] = React.useState(false)
  const [modelsProvider, setModelsProvider] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [modelsDialogOpen, setModelsDialogOpen] = React.useState(false)
  const [busyProviderId, setBusyProviderId] = React.useState<string | null>(null)
  const [providerToRemove, setProviderToRemove] = React.useState<PublicAiProviderDescriptor | null>(null)
  const [isolatedProviderToRemove, setIsolatedProviderToRemove] = React.useState<string | null>(null)
  const [error, setError] = React.useState<string | null>(null)

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
  }, [refresh])

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
    setBusyProviderId(provider.id)
    setError(null)
    try {
      setSnapshot(await removePublicAiCustomProvider(provider.id))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyProviderId(null)
      setProviderToRemove(null)
    }
  }, [])

  // removeIsolatedProvider deletes one exact failed standalone reference without fabricating a descriptor.
  // removeIsolatedProvider 不伪造描述符，直接删除一项精确失败的独立引用。
  const removeIsolatedProvider = React.useCallback(async (providerId: string) => {
    setBusyProviderId(providerId)
    setError(null)
    try {
      setSnapshot(await removePublicAiCustomProvider(providerId))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setBusyProviderId(null)
      setIsolatedProviderToRemove(null)
    }
  }, [])

  // systemProviders contains the immutable built-in provider definitions.
  // systemProviders 包含不可变的内置供应商定义。
  const systemProviders = snapshot?.providers.filter((provider) => provider.kind === "system") ?? []
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
    <div className="settings-direct-content provider-management-surface space-y-6 px-4 lg:px-6">
      <div>
        <h2 className="text-xl font-semibold">供应商管理</h2>
        <p className="text-sm text-muted-foreground">
          管理 Vulcan Code 已实现的内置供应商与复用现有协议适配器的自定义供应商。
        </p>
      </div>

      {error ? (
        <div className="rounded-xl border border-destructive/40 p-4 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {customProviderManifestFailureTotalCount > 0 ? (
        <div className="rounded-xl border border-destructive/40 bg-destructive/5 p-4 text-sm">
          <p className="font-semibold text-destructive">
            已隔离 {customProviderManifestFailureTotalCount} 个自定义供应商
          </p>
          <p className="mt-1 text-muted-foreground">
            该供应商已停止参与运行时路由；其他供应商及其设置仍可正常使用和保存。下列独立 Manifest 源文件会在保存时保持原始字节不变；可修复源文件后重启 Service，或显式删除对应隔离项。
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
                  删除隔离项
                </Button>
              </li>
            ))}
          </ul>
          {snapshot?.customProviderManifestFailuresTruncated ? (
            <p className="mt-2 text-xs text-muted-foreground">其余诊断已按安全上限省略。</p>
          ) : null}
        </div>
      ) : null}

      <ProviderManifestStatusPanel />

      <CustomProviderTable
        busyProviderId={busyProviderId}
        connections={snapshot?.connections ?? []}
        customModels={snapshot?.customModels ?? {}}
        loading={!snapshot}
        onCreate={openCreate}
        onEdit={openEdit}
        onManageModels={openModels}
        onRemove={setProviderToRemove}
        providers={customProviders}
      />

      <SystemProviderTable
        connections={snapshot?.connections ?? []}
        loading={!snapshot}
        providers={systemProviders}
      />

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
      <AlertDialog onOpenChange={(open) => !open && setProviderToRemove(null)} open={providerToRemove !== null}>
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
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
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
        onOpenChange={(open) => !open && setIsolatedProviderToRemove(null)}
        open={isolatedProviderToRemove !== null}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除隔离的自定义供应商</AlertDialogTitle>
            <AlertDialogDescription>
              <span className="block space-y-2">
                <span className="block">
                  将永久删除该供应商的独立 Manifest、中央引用、连接、凭据与模型设置；其他供应商不受影响。
                </span>
                <span className="block break-all font-mono text-foreground">
                  {isolatedProviderToRemove}
                </span>
              </span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
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
              永久删除
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

// CustomProviderTable renders the Model Core custom-provider table structure.
// CustomProviderTable 渲染 Model Core 的自定义供应商表格结构。
function CustomProviderTable(props: {
  busyProviderId: string | null
  connections: PublicAiSettingsSnapshot["connections"]
  customModels: NonNullable<PublicAiSettingsSnapshot["customModels"]>
  loading: boolean
  onCreate: () => void
  onEdit: (provider: PublicAiProviderDescriptor) => void
  onManageModels: (provider: PublicAiProviderDescriptor) => void
  onRemove: (provider: PublicAiProviderDescriptor) => void
  providers: PublicAiProviderDescriptor[]
}) {
  const { t } = useI18n()
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <ServerCog className="size-4" />
          <h3 className="font-medium">自定义供应商</h3>
          <Badge variant="secondary">{props.providers.length}</Badge>
        </div>
        <Button
          type="button"
          size="sm"
          className="inline-flex items-center gap-1.5 leading-none"
          onClick={props.onCreate}
        >
          <Plus className="size-4" />
          <span className="leading-none">添加供应商</span>
        </Button>
      </div>

      {props.loading ? (
        <div className="flex items-center gap-2 rounded-lg border p-4 text-sm text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          正在加载供应商
        </div>
      ) : props.providers.length === 0 ? (
        <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
          尚未添加自定义供应商。
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border">
          <Table>
            <TableHeader className="bg-muted/40">
              <TableRow>
                <TableHead className="w-[18rem]">供应商</TableHead>
                <TableHead>接口</TableHead>
                <TableHead className="w-32">资源数量</TableHead>
                <TableHead className="w-28 text-center">状态</TableHead>
                <TableHead className="w-24 text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {props.providers.map((provider) => {
                // credentialCount counts credentials explicitly bound to this provider.
                // credentialCount 统计明确绑定到此供应商的凭据。
                const credentialCount = props.connections.filter(
                  (connection) => connection.provider_id === provider.id,
                ).length
                // modelCount counts provider-owned model overlays independently from credentials.
                // modelCount 独立于凭据统计供应商自有模型覆盖。
                const modelCount = Object.keys(props.customModels[provider.id] ?? {}).length
                // protocolNames is the unique protocol set declared by all service entries.
                // protocolNames 是所有服务入口声明的唯一协议集合。
                const protocolNames = uniqueProtocolNames(provider)
                return (
                  <TableRow key={provider.id}>
                    <TableCell className="whitespace-normal py-3 align-top">
                      <div className="flex items-center gap-3">
                        <ProviderIcon category={provider.category} className="size-[26px]" providerId={provider.id} />
                        <div className="min-w-0">
                          <span className="font-medium">{provider.display_name}</span>
                          <div className="mt-1 truncate font-mono text-xs text-muted-foreground">
                            {provider.id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="whitespace-normal py-3 align-top">
                      <div className="font-medium">
                        {protocolNames.length > 0 ? protocolNames.join(" · ") : "未声明调用协议"}
                      </div>
                      <div className="mt-1 space-y-1 text-xs text-muted-foreground">
                        {provider.service_entries.map((entry) => (
                          <div className="break-all font-mono" key={entry.id}>
                            {entry.base_url}
                          </div>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell className="whitespace-normal py-3 align-top text-sm text-muted-foreground">
                      <div>{t("settings.customProviders.modelCount").replace("{count}", String(modelCount))}</div>
                      <div className="mt-1">{credentialCount} 个凭据</div>
                    </TableCell>
                    <TableCell className="py-3 text-center align-top">
                      <Badge variant={credentialCount > 0 ? "default" : "secondary"}>
                        {credentialCount > 0 ? "已配置" : "未配置"}
                      </Badge>
                    </TableCell>
                    <TableCell className="py-3 text-right align-top">
                      <div className="flex justify-end gap-1">
                        <Button
                          aria-label={t("settings.customProviders.manageModelsFor").replace("{provider}", provider.display_name)}
                          onClick={() => props.onManageModels(provider)}
                          size="icon-sm"
                          title={t("settings.customProviders.manageModels")}
                          type="button"
                          variant="outline"
                        >
                          <ListTree className="size-4" />
                        </Button>
                        <Button
                          aria-label={`编辑 ${provider.display_name}`}
                          onClick={() => props.onEdit(provider)}
                          size="icon-sm"
                          type="button"
                          variant="outline"
                        >
                          <Pencil className="size-4" />
                        </Button>
                        <Button
                          aria-label={`移除 ${provider.display_name}`}
                          disabled={props.busyProviderId === provider.id}
                          onClick={() => props.onRemove(provider)}
                          size="icon-sm"
                          type="button"
                          variant="ghost"
                          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                        >
                          {props.busyProviderId === provider.id ? (
                            <LoaderCircle className="size-4 animate-spin" />
                          ) : (
                            <Trash2 className="size-4" />
                          )}
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  )
}

// SystemProviderTable renders the Model Core native-provider table structure.
// SystemProviderTable 渲染 Model Core 的内置供应商表格结构。
function SystemProviderTable(props: {
  connections: PublicAiSettingsSnapshot["connections"]
  loading: boolean
  providers: PublicAiProviderDescriptor[]
}) {
  const { t } = useI18n()
  // orderedProviders keeps normal built-ins first and promotional entries last.
  // orderedProviders 将常规内置供应商排在前面，将推广入口排在最后。
  const orderedProviders = React.useMemo(() => props.providers
    .map((provider, index) => ({ provider, index }))
    .sort((left, right) => (
      Number(Boolean(left.provider.promotion)) - Number(Boolean(right.provider.promotion))
      || left.index - right.index
    ))
    .map(({ provider }) => provider), [props.providers])
  return (
    <section className="space-y-3">
      <div className="flex items-center gap-2">
        <LockKeyhole className="size-4" />
        <h3 className="font-medium">内置供应商</h3>
        <Badge variant="secondary">{props.providers.length}</Badge>
      </div>

      {props.loading ? (
        <div className="flex items-center gap-2 rounded-lg border p-4 text-sm text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          正在加载供应商
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border">
          <Table>
            <TableHeader className="bg-muted/40">
              <TableRow>
                <TableHead className="w-[18rem]">供应商</TableHead>
                <TableHead>详情</TableHead>
                <TableHead className="w-32 text-right">配置状态</TableHead>
                <TableHead className="w-20 text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orderedProviders.map((provider) => {
                // credentialCount counts credentials explicitly bound to this provider.
                // credentialCount 统计明确绑定到此供应商的凭据。
                const credentialCount = props.connections.filter(
                  (connection) => connection.provider_id === provider.id,
                ).length
                // protocolNames is the unique protocol set declared by all service entries.
                // protocolNames 是所有服务入口声明的唯一协议集合。
                const protocolNames = uniqueProtocolNames(provider)
                // providerDescription is the registered bilingual introduction for this provider.
                // providerDescription 是当前供应商已注册的双语介绍。
                const providerDescription = localizedProviderDescription(provider.id, t)
                return (
                  <TableRow key={provider.id}>
                    <TableCell className="whitespace-normal py-3 align-top">
                      <div className="flex items-center gap-3">
                        <ProviderIcon category={provider.category} className="size-[26px]" providerId={provider.id} />
                        <div className="min-w-0">
                          <span className="font-medium">{provider.display_name}</span>
                          <div className="mt-1 truncate font-mono text-xs text-muted-foreground">
                            {provider.id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="whitespace-normal py-3 align-top">
                      <div className="text-sm text-muted-foreground">
                        {providerDescription}
                      </div>
                      <div className="mt-2 flex flex-wrap gap-2">
                        <ProviderSummaryTooltip
                          items={provider.service_entries.map((entry) => entry.display_name)}
                          label={t("settings.providerSummary.serviceEntries").replace(
                            "{count}",
                            String(provider.service_entries.length),
                          )}
                          title={t("settings.providerSummary.serviceEntriesTitle")}
                        />
                        <ProviderSummaryTooltip
                          items={protocolNames}
                          label={t("settings.providerSummary.protocols").replace(
                            "{count}",
                            String(protocolNames.length),
                          )}
                          title={t("settings.providerSummary.protocolsTitle")}
                        />
                      </div>
                    </TableCell>
                    <TableCell className="py-3 text-right align-top">
                      <Badge variant={credentialCount > 0 ? "default" : "secondary"}>
                        {credentialCount > 0 ? `${credentialCount} 个凭据` : "未配置"}
                      </Badge>
                    </TableCell>
                    <TableCell className="py-3 text-right align-top">
                      <span className="text-xs text-muted-foreground">
                        {t("settings.models.liveModels")}
                      </span>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
      )}
    </section>
  )
}

// CustomProtocolRouteDraftCard edits one enabled protocol's route overrides: base URL
// override, endpoint path, authentication, discovery, and fixed headers. Empty values
// inherit the shared provider projection on save (plan §R6.5).
// CustomProtocolRouteDraftCard 编辑一个启用协议的路由覆盖：基础 URL 覆盖、端点路径、
// 鉴权、发现与固定请求头。保存时空值继承共享供应商投影（方案 §R6.5）。
function CustomProtocolRouteDraftCard(props: {
  draft: CustomProtocolRouteDraft
  onChange: (draft: CustomProtocolRouteDraft) => void
  protocol: PublicAiCustomProviderProtocol
  t: (key: TranslationKey) => string
}) {
  const { draft, onChange, protocol, t } = props
  return (
    <details className="rounded-lg border bg-muted/30 p-3 text-sm">
      <summary className="cursor-pointer font-medium text-foreground">
        {t("settings.customProviders.routeDraftFor").replace(
          "{protocol}",
          protocolDisplayName(customProtocolProfileId(protocol)),
        )}
      </summary>
      <div className="mt-3 grid gap-3">
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeBaseUrl")}</span>
          <Input
            onChange={(event) => onChange({ ...draft, baseUrlOverride: event.target.value })}
            placeholder={t("settings.customProviders.routeBaseUrlPlaceholder")}
            value={draft.baseUrlOverride}
          />
        </label>
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeEndpointPath")}</span>
          <Input
            onChange={(event) => onChange({ ...draft, endpointPath: event.target.value })}
            placeholder="/v1/chat/completions"
            value={draft.endpointPath}
          />
        </label>
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeAuthentication")}</span>
            <Select
              onValueChange={(value) => onChange({
                ...draft,
                authentication: customProtocolRouteDraftValue(value) as CustomProtocolRouteDraft["authentication"],
              })}
              value={customProtocolRouteSelectValue(draft.authentication)}
            >
              <SelectTrigger><SelectValue placeholder={t("settings.customProviders.routeInherit")} /></SelectTrigger>
              <SelectContent>
                <SelectItem value={CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE}>
                  {t("settings.customProviders.routeInherit")}
                </SelectItem>
                <SelectItem value="bearer_authorization">Authorization: Bearer</SelectItem>
                <SelectItem value="google_api_key">Gemini API Key</SelectItem>
                <SelectItem value="x_api_key">x-api-key</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeDiscoveryMode")}</span>
            <Select
              onValueChange={(value) => onChange({
                ...draft,
                modelDiscoveryMode: customProtocolRouteDraftValue(value) as CustomProtocolRouteDraft["modelDiscoveryMode"],
              })}
              value={customProtocolRouteSelectValue(draft.modelDiscoveryMode)}
            >
              <SelectTrigger><SelectValue placeholder={t("settings.customProviders.routeInherit")} /></SelectTrigger>
              <SelectContent>
                <SelectItem value={CUSTOM_PROTOCOL_ROUTE_INHERIT_SELECT_VALUE}>
                  {t("settings.customProviders.routeInherit")}
                </SelectItem>
                <SelectItem value="standard">{t("settings.customProviders.discoveryStandard")}</SelectItem>
                <SelectItem value="custom_path">{t("settings.customProviders.discoveryCustomPath")}</SelectItem>
                <SelectItem value="manual">{t("settings.customProviders.discoveryManual")}</SelectItem>
              </SelectContent>
            </Select>
          </label>
        </div>
        {draft.modelDiscoveryMode === "custom_path" ? (
          <label className="settings-form-field">
            <span>{t("settings.customProviders.routeDiscoveryPath")}</span>
            <Input
              onChange={(event) => onChange({ ...draft, modelDiscoveryPath: event.target.value })}
              placeholder="/models"
              value={draft.modelDiscoveryPath}
            />
          </label>
        ) : null}
        <label className="settings-form-field">
          <span>{t("settings.customProviders.routeHeaders")}</span>
          <textarea
            className="min-h-[72px] w-full rounded-md border bg-background px-3 py-2 text-xs"
            onChange={(event) => {
              const headers: Record<string, string> = {}
              for (const line of event.target.value.split("\n")) {
                const separator = line.indexOf(":")
                if (separator > 0) {
                  const key = line.slice(0, separator).trim()
                  const value = line.slice(separator + 1).trim()
                  if (key) headers[key] = value
                }
              }
              onChange({ ...draft, requestHeaders: headers })
            }}
            placeholder="accept: application/json"
            value={Object.entries(draft.requestHeaders)
              .map(([key, value]) => `${key}: ${value}`)
              .join("\n")}
          />
        </label>
      </div>
    </details>
  )
}

// CustomProviderDialog edits the exact public custom-provider contract.
// CustomProviderDialog 编辑精确的公开自定义供应商契约。
function CustomProviderDialog(props: {
  connections: PublicAiConnection[]
  modelOverrideRecords: PublicAiModelOverrideRecord[]
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  provider: PublicAiProviderDescriptor | null
}) {
  const { t } = useI18n()
  const [displayName, setDisplayName] = React.useState("")
  const [baseUrl, setBaseUrl] = React.useState("")
  // protocols is the authoritative enabled protocol collection (at least one element) and
  // routeDrafts keeps one independent draft per enabled protocol (plan §R6.4/§R6.5).
  // protocols 是权威启用协议集合（至少一个元素），routeDrafts 为每个启用协议保留独立草稿
  // （方案 §R6.4/§R6.5）。
  const [protocols, setProtocols] = React.useState<PublicAiCustomProviderProtocol[]>(["open_ai_responses"])
  const [routeDrafts, setRouteDrafts] = React.useState<CustomProviderProtocolCollection["drafts"]>({})
  const [remoteCompactionPolicy, setRemoteCompactionPolicy] = React.useState<PublicAiResponsesRemoteCompactionPolicy>("disabled")
  const [websocketPolicy, setWebsocketPolicy] = React.useState<PublicAiResponsesWebsocketPolicy>("disabled")
  const [authentication, setAuthentication] = React.useState<"bearer_authorization" | "google_api_key" | "x_api_key">("bearer_authorization")
  const [modelDiscoveryMode, setModelDiscoveryMode] = React.useState<PublicAiCustomProviderModelDiscoveryMode>("standard")
  const [modelDiscoveryPath, setModelDiscoveryPath] = React.useState("")
  const [credentialDisplayName, setCredentialDisplayName] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [validationConnectionId, setValidationConnectionId] = React.useState("")
  const [step, setStep] = React.useState<"connection" | "credential" | "result">("connection")
  const [probeResult, setProbeResult] = React.useState<PublicAiCustomProviderProbeResult | null>(null)
  const [probing, setProbing] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [pendingConfirmation, setPendingConfirmation] = React.useState<"manual" | "pending" | null>(null)
  // protocolOverrideConfirmation retains the original save intent while incompatible route settings are confirmed.
  // protocolOverrideConfirmation 在确认不兼容路由设置期间保留原始保存意图。
  const [protocolOverrideConfirmation, setProtocolOverrideConfirmation] = React.useState<"ready" | "manual" | "pending" | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // providerConnections is the complete account set owned by the edited provider.
  // providerConnections 是被编辑供应商持有的完整账号集合。
  const providerConnections = React.useMemo(() => props.connections.filter(
    (connection) => connection.provider_id === props.provider?.id,
  ), [props.connections, props.provider?.id])
  // initializedDialogIdentity prevents background snapshots from erasing an in-progress provider edit.
  // initializedDialogIdentity 防止后台快照清除正在进行的供应商编辑。
  const initializedDialogIdentity = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (!props.open) {
      initializedDialogIdentity.current = null
      return
    }
    // dialogIdentity distinguishes provider creation from each concrete provider editor.
    // dialogIdentity 区分供应商新建窗口与每一个具体供应商编辑器。
    const dialogIdentity = props.provider?.id ?? "__new_custom_provider__"
    if (initializedDialogIdentity.current === dialogIdentity) {
      return
    }
    initializedDialogIdentity.current = dialogIdentity
    const entry = props.provider?.service_entries[0]
    setDisplayName(props.provider?.display_name ?? "")
    setBaseUrl(entry?.base_url ?? "")
    setAuthentication(entry?.authentication === "google_api_key"
      ? "google_api_key"
      : entry?.authentication === "x_api_key" ? "x_api_key" : "bearer_authorization")
    // The persisted protocol collection and its per-protocol route drafts initialize the
    // editable state so an edit never drops a route-level override (plan §R6.5).
    // 持久化协议集合及其逐协议路由草稿初始化可编辑状态，使编辑绝不丢弃路由级覆盖
    // （方案 §R6.5）。
    const collection = protocolCollectionFromEntry(entry)
    setProtocols(collection.protocols)
    setRouteDrafts(collection.drafts)
    const codexFeatures = codexResponsesFeaturePoliciesFromEntry(entry)
    setRemoteCompactionPolicy(codexFeatures.remoteCompactionPolicy)
    setWebsocketPolicy(codexFeatures.websocketPolicy)
    setModelDiscoveryMode(customProviderDiscoveryMode(entry))
    const firstDraft = collection.drafts[collection.protocols[0]]
    setModelDiscoveryPath(firstDraft?.modelDiscoveryPath ?? "")
    setCredentialDisplayName(props.provider?.display_name ?? "")
    setSecret("")
    setValidationConnectionId(providerConnections[0]?.id ?? "")
    setStep("connection")
    setProbeResult(null)
    setPendingConfirmation(null)
    setProtocolOverrideConfirmation(null)
    setError(null)
  }, [props.open, props.provider, providerConnections])

  React.useEffect(() => {
    if (!props.open || !props.provider) {
      return
    }
    setValidationConnectionId((current) => (
      providerConnections.some((connection) => connection.id === current)
        ? current
        : (providerConnections[0]?.id ?? "")
    ))
  }, [props.open, props.provider, providerConnections])

  // validateConnectionDraft validates the complete locally editable endpoint contract.
  // validateConnectionDraft 校验完整的本地可编辑入口契约。
  const validateConnectionDraft = React.useCallback((): boolean => {
    const validationError = validateCustomProviderConnectionStep({
      baseUrl,
      displayName,
      modelDiscoveryMode,
      modelDiscoveryPath,
    })
    if (validationError) {
      setError(t(validationError))
      return false
    }
    setError(null)
    return true
  }, [baseUrl, displayName, modelDiscoveryMode, modelDiscoveryPath, t])

  // continueToCredential validates the endpoint contract before asking for a secret.
  // continueToCredential 在请求秘密前校验入口契约。
  const continueToCredential = React.useCallback(() => {
    if (!validateConnectionDraft()) {
      return
    }
    setProbeResult(null)
    setStep("credential")
  }, [validateConnectionDraft])

  // requestBody builds the exact typed service request from the current form.
  // requestBody 从当前表单构造精确的类型化服务请求。
  const requestBody = React.useCallback((
    acceptance?: "manual" | "pending",
    confirmInactiveProtocolOverrideDeletion = false,
  ): PublicAiCustomProviderSaveRequest => ({
    providerId: props.provider?.id,
    displayName: displayName.trim(),
    baseUrl: baseUrl.trim(),
    // The enabled protocol collection is authoritative and never empty.
    // 启用协议集合是权威事实且绝不为空。
    protocols,
    remoteCompactionPolicy,
    websocketPolicy,
    authentication,
    ...(props.provider
      ? (validationConnectionId ? { validationConnectionId } : {})
      : {
          credentialDisplayName: credentialDisplayName.trim(),
          secret: secret.trim(),
        }),
    modelDiscoveryMode,
    modelDiscoveryPath: modelDiscoveryMode === "custom_path" ? modelDiscoveryPath.trim() : undefined,
    // Per-protocol route overrides are serialized from the independent drafts so edits never
    // silently drop route-level base URLs, authentication, discovery, or headers (plan
    // §R6.5/§R6.10).
    // 逐协议路由覆盖从独立草稿序列化，使编辑绝不静默丢弃路由级基础 URL、鉴权、发现或
    // 请求头（方案 §R6.5/§R6.10）。
    protocolRoutes: protocolRoutesFromCollection({ protocols, drafts: routeDrafts }),
    allowManualFallback: acceptance === "manual",
    allowPendingValidation: acceptance === "pending",
    confirmInactiveProtocolOverrideDeletion,
  }), [authentication, baseUrl, credentialDisplayName, displayName, modelDiscoveryMode, modelDiscoveryPath, protocols, remoteCompactionPolicy, routeDrafts, secret, validationConnectionId, websocketPolicy])

  // probe verifies authentication and model discovery without persisting the provider or secret.
  // probe 在不持久化供应商或秘密的情况下验证鉴权与模型发现。
  const probe = React.useCallback(async () => {
    if (props.provider && !validationConnectionId) {
      setError(t("settings.customProviders.validationCredentialRequired"))
      return
    }
    if (!props.provider && (!credentialDisplayName.trim() || !secret.trim())) {
      setError(t("settings.customProviders.credentialRequired"))
      return
    }
    setProbing(true)
    setError(null)
    try {
      setProbeResult(await probePublicAiCustomProvider(requestBody()))
      setStep("result")
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setProbing(false)
    }
  }, [credentialDisplayName, props.provider, requestBody, secret, t, validationConnectionId])

  // originalEntry is the persisted technical contract used to distinguish cosmetic edits.
  // originalEntry 是用于区分纯展示编辑的持久化技术契约。
  const originalEntry = props.provider?.service_entries[0]
  // originalProtocols is the exact persisted protocol set projected into the browser-editable
  // contract (plan §R6.8).
  // originalProtocols 是投影到浏览器可编辑契约中的精确持久化协议集合（方案 §R6.8）。
  const originalProtocols = protocolCollectionFromEntry(originalEntry).protocols
  // protocolSetChange distinguishes destructive protocol removals from other technical edits;
  // additions never invalidate existing settings (plan §R6.8).
  // protocolSetChange 将破坏性协议删除与其他技术编辑区分开；新增绝不让既有设置失效
  // （方案 §R6.8）。
  const protocolChange = protocolSetChange(originalProtocols, protocols)
  const removedProtocols = protocolChange.removed
  // inactiveProtocolOverrideCount counts exact settings owned by the removed protocols that
  // require explicit deletion confirmation.
  // inactiveProtocolOverrideCount 统计被移除协议拥有的、需要显式确认删除的精确设置数量。
  const inactiveProtocolOverrideCount = props.provider && removedProtocols.length > 0
    ? props.modelOverrideRecords.filter((record) => (
        record.providerId === props.provider?.id
        && removedProtocols.some((removed) => customProtocolProfileId(removed) === record.protocolId)
      )).length
    : 0

  // save persists only the explicit action represented by the result screen.
  // save 仅持久化结果页明确表示的操作。
  const save = React.useCallback(async (
    acceptance?: "manual" | "pending",
    confirmInactiveProtocolOverrideDeletion = false,
  ) => {
    if (inactiveProtocolOverrideCount > 0 && !confirmInactiveProtocolOverrideDeletion) {
      setPendingConfirmation(null)
      setProtocolOverrideConfirmation(acceptance ?? "ready")
      return
    }
    setSaving(true)
    setError(null)
    try {
      await persistAndApplyCustomProviderSnapshot(
        requestBody(acceptance, confirmInactiveProtocolOverrideDeletion),
        savePublicAiCustomProvider,
        props.onSaved,
      )
      props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [inactiveProtocolOverrideCount, props, requestBody])

  // saveCredentialFreeEdit persists a valid provider contract when no account exists to probe.
  // saveCredentialFreeEdit 在不存在可探测账号时持久化有效的供应商契约。
  const saveCredentialFreeEdit = React.useCallback(() => {
    if (!validateConnectionDraft()) {
      return
    }
    void save()
  }, [save, validateConnectionDraft])

  // primaryProtocol is the first enabled protocol used for the shared endpoint preview and
  // the shared Codex capability switches; every protocol still keeps its own route draft.
  // primaryProtocol 是用于共享端点预览与共享 Codex 能力开关的第一个启用协议；每个协议
  // 仍保留自己的路由草稿。
  const primaryProtocol = protocols[0] ?? "open_ai_responses"
  const protocolEndpoint = customProviderEndpointPreview(baseUrl, primaryProtocol)
  const discoveryEndpoint = customProviderDiscoveryEndpointPreview(baseUrl, modelDiscoveryMode, modelDiscoveryPath)
  const acceptedWithoutOverride = probeResult?.status.state === "ready"
  const validationConnection = providerConnections.find(
    (connection) => connection.id === validationConnectionId,
  )
  // connectionContractChanged mirrors the backend service-entry comparison for user-facing routing.
  // connectionContractChanged 镜像后端服务入口比较，用于决定界面流程。
  const connectionContractChanged = Boolean(props.provider) && (
    !originalEntry
    || baseUrl.trim().replace(/\/+$/, "") !== originalEntry.base_url.replace(/\/+$/, "")
    || protocolCollectionChanged(originalEntry, { protocols, drafts: routeDrafts })
    || authentication !== customProviderAuthentication(originalEntry)
    || modelDiscoveryMode !== customProviderDiscoveryMode(originalEntry)
    || (modelDiscoveryMode === "custom_path"
      && modelDiscoveryPath.trim() !== (protocolCollectionFromEntry(originalEntry).drafts[originalProtocols[0]]?.modelDiscoveryPath ?? ""))
  )
  // responsesFeaturesChanged tracks explicit Codex transport capabilities without forcing credential revalidation.
  // responsesFeaturesChanged 跟踪显式 Codex 传输能力开关，但不会强制重新验证凭据。
  const originalResponsesFeatures = originalEntry?.protocol_routes.find(
    (route) => route.protocol_profile_id === "openai-codex-responses.v1",
  )?.responses_features
  const responsesFeaturesChanged = Boolean(props.provider)
    && protocols.includes("open_ai_codex_responses")
    && (
      remoteCompactionPolicy !== (originalResponsesFeatures?.remote_compaction ?? "disabled")
      || websocketPolicy !== (originalResponsesFeatures?.websocket ?? "disabled")
    )
  // credentialFreeEdit identifies an existing provider whose contract cannot be remotely probed yet.
  // credentialFreeEdit 标识当前尚无法远程探测契约的无账号现有供应商。
  const credentialFreeEdit = Boolean(props.provider) && providerConnections.length === 0
  // providerChanged prevents an unchanged editor from starting unnecessary validation work.
  // providerChanged 防止未发生变化的编辑器启动不必要的验证工作。
  const providerChanged = Boolean(props.provider) && (
    displayName.trim() !== props.provider?.display_name
    || connectionContractChanged
    || responsesFeaturesChanged
  )
  // directSaveOnly covers display and capability changes that do not change the credential route contract.
  // directSaveOnly 覆盖不改变凭据路由契约的展示信息与能力开关变更。
  const directSaveOnly = Boolean(props.provider) && providerChanged && !connectionContractChanged

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open && (probing || saving)) {
          return
        }
        props.onOpenChange(open)
      }}
      open={props.open}
    >
      <DialogContent className="max-h-[min(760px,calc(100vh-2rem))] overflow-y-auto sm:max-w-[620px]">
        <DialogHeader>
          <DialogTitle>{t(props.provider ? "settings.customProviders.editTitle" : "settings.customProviders.addTitle")}</DialogTitle>
          <DialogDescription>
            {t("settings.customProviders.dialogDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-3 gap-2" aria-label={t("settings.customProviders.progressLabel")}>
          {(["connection", "credential", "result"] as const).map((candidate, index) => (
            <div className={`rounded-lg border px-3 py-2 text-xs ${candidate === step ? "border-foreground bg-muted font-medium" : "text-muted-foreground"}`} key={candidate}>
              {index + 1}. {t(customProviderStepLabelKeys[candidate])}
            </div>
          ))}
        </div>

        {step === "connection" ? (
          <div className="grid gap-4">
            <label className="settings-form-field">
              <span>{t("settings.customProviders.displayName")}</span>
              <Input onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
            </label>
            <label className="settings-form-field">
              <span>{t("settings.customProviders.baseUrl")}</span>
              <Input onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://example.com/v1" value={baseUrl} />
            </label>
            <div className="grid gap-4">
              <fieldset className="settings-form-field">
                <legend className="text-sm font-medium">{t("settings.customProviders.protocols")}</legend>
                <p className="text-xs text-muted-foreground">{t("settings.customProviders.protocolsHint")}</p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {customProtocolOptions.map((option) => {
                    const enabled = protocols.includes(option.id)
                    return (
                      <label
                        className={`inline-flex cursor-pointer items-center gap-2 rounded-full border px-3 py-1.5 text-sm ${enabled ? "border-foreground bg-muted font-medium" : "text-muted-foreground"}`}
                        key={option.id}
                      >
                        <Checkbox
                          checked={enabled}
                          disabled={saving}
                          onCheckedChange={() => {
                            setProtocols((current) => {
                              const next = enabled
                                ? current.filter((protocol) => protocol !== option.id)
                                : [...current, option.id]
                              // At least one protocol must stay enabled; the last protocol
                              // cannot be removed (plan §R6.4).
                              // 至少保留一个启用协议；最后一个协议不可移除（方案 §R6.4）。
                              if (next.length === 0) {
                                return current
                              }
                              if (!enabled) {
                                setRouteDrafts((drafts: CustomProviderProtocolCollection["drafts"]) => ({
                                  ...drafts,
                                  [option.id]: drafts[option.id] ?? emptyProtocolRouteDraft(),
                                }))
                              }
                              return sortProtocolsByAutoPriority(next)
                            })
                          }}
                        />
                        {option.label}
                      </label>
                    )
                  })}
                </div>
              </fieldset>
              <label className="settings-form-field">
                <span>{t("settings.customProviders.authentication")}</span>
                <Select onValueChange={(value) => setAuthentication(value as "bearer_authorization" | "google_api_key" | "x_api_key")} value={authentication}>
                  <SelectTrigger><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="bearer_authorization">Authorization: Bearer</SelectItem>
                    <SelectItem value="google_api_key">Gemini API Key</SelectItem>
                    <SelectItem value="x_api_key">x-api-key</SelectItem>
                  </SelectContent>
                </Select>
              </label>
            </div>
            {protocols.length > 1 ? (
              <fieldset className="settings-form-field">
                <legend className="text-sm font-medium">{t("settings.customProviders.routeDrafts")}</legend>
                <p className="text-xs text-muted-foreground">{t("settings.customProviders.routeDraftsHint")}</p>
                <div className="mt-2 grid gap-2">
                  {protocols.map((protocolId) => (
                    <CustomProtocolRouteDraftCard
                      draft={routeDrafts[protocolId] ?? emptyProtocolRouteDraft()}
                      key={protocolId}
                      onChange={(draft) => setRouteDrafts((drafts: CustomProviderProtocolCollection["drafts"]) => ({ ...drafts, [protocolId]: draft }))}
                      protocol={protocolId}
                      t={t}
                    />
                  ))}
                </div>
              </fieldset>
            ) : null}
            <label className="settings-form-field">
              <span>{t("settings.customProviders.discoveryMode")}</span>
              <Select onValueChange={(value) => setModelDiscoveryMode(value as PublicAiCustomProviderModelDiscoveryMode)} value={modelDiscoveryMode}>
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="standard">{t("settings.customProviders.discoveryStandard")}</SelectItem>
                  <SelectItem value="custom_path">{t("settings.customProviders.discoveryCustomPath")}</SelectItem>
                  <SelectItem value="manual">{t("settings.customProviders.discoveryManual")}</SelectItem>
                </SelectContent>
              </Select>
            </label>
            {modelDiscoveryMode === "custom_path" ? (
              <label className="settings-form-field">
                <span>{t("settings.customProviders.discoveryPath")}</span>
                <Input onChange={(event) => setModelDiscoveryPath(event.target.value)} placeholder="/models" value={modelDiscoveryPath} />
              </label>
            ) : null}
            {protocols.includes("open_ai_codex_responses") ? (
              <div className="grid gap-3 rounded-xl border bg-muted/30 p-3 text-sm">
                <p className="font-medium text-foreground">{t("settings.customProviders.codexCapabilities")}</p>
                <label className="flex items-start gap-2">
                  <Switch
                    checked={remoteCompactionPolicy === "enabled_with_local_fallback"}
                    className="mt-1"
                    onCheckedChange={(checked) => setRemoteCompactionPolicy(checked ? "enabled_with_local_fallback" : "disabled")}
                  />
                  <span>
                    <span className="block font-medium">{t("settings.customProviders.codexRemoteCompaction")}</span>
                    <span className="block text-xs text-muted-foreground">{t("settings.customProviders.codexRemoteCompactionHint")}</span>
                  </span>
                </label>
                <label className="flex items-start gap-2">
                  <Switch
                    checked={websocketPolicy !== "disabled"}
                    className="mt-1"
                    onCheckedChange={(checked) => setWebsocketPolicy(checked ? "unverified_override" : "disabled")}
                  />
                  <span>
                    <span className="block font-medium">{t("settings.customProviders.codexWebsocket")}</span>
                    <span className="block text-xs text-muted-foreground">{t("settings.customProviders.codexWebsocketHint")}</span>
                  </span>
                </label>
              </div>
            ) : null}
            <div className="rounded-xl border bg-muted/30 p-3 text-xs text-muted-foreground">
              <p><span className="font-medium text-foreground">{t("settings.customProviders.protocolEndpoint")}</span> {protocolEndpoint || "—"}</p>
              <p className="mt-1"><span className="font-medium text-foreground">{t("settings.customProviders.modelsEndpoint")}</span> {discoveryEndpoint || t("settings.customProviders.manualOnly")}</p>
            </div>
            {credentialFreeEdit ? (
              <Alert>
                <CircleAlert className="size-4" />
                <AlertTitle>{t("settings.customProviders.credentialFreeEditTitle")}</AlertTitle>
                <AlertDescription>{t("settings.customProviders.credentialFreeEditDescription")}</AlertDescription>
              </Alert>
            ) : null}
          </div>
        ) : null}

        {step === "credential" ? (
          <div className="grid gap-4">
            <Alert>
              <CircleAlert className="size-4" />
              <AlertTitle>{t("settings.customProviders.validationTitle")}</AlertTitle>
              <AlertDescription>{t(props.provider ? "settings.customProviders.validationEditDescription" : "settings.customProviders.validationCreateDescription")}</AlertDescription>
            </Alert>
            {props.provider ? (
              <label className="settings-form-field">
                <span>{t("settings.customProviders.validationCredential")}</span>
                <Select onValueChange={setValidationConnectionId} value={validationConnectionId}>
                  <SelectTrigger><SelectValue placeholder={t("settings.customProviders.validationCredentialRequired")} /></SelectTrigger>
                  <SelectContent>
                    {providerConnections.map((connection) => (
                      <SelectItem key={connection.id} value={connection.id}>
                        {connection.display_name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </label>
            ) : (
              <>
                <label className="settings-form-field">
                  <span>{t("settings.customProviders.firstCredentialName")}</span>
                  <Input onChange={(event) => setCredentialDisplayName(event.target.value)} value={credentialDisplayName} />
                </label>
                <label className="settings-form-field">
                  <span>{t("settings.customProviders.apiKey")}</span>
                  <Input autoComplete="off" onChange={(event) => setSecret(event.target.value)} placeholder={t("settings.customProviders.apiKeyCreatePlaceholder")} type="password" value={secret} />
                </label>
              </>
            )}
          </div>
        ) : null}

        {step === "result" && probeResult ? (
          <div className="grid gap-4">
            <Alert variant={probeResult.status.state === "authentication_failed" ? "destructive" : "default"}>
              {acceptedWithoutOverride ? <CircleCheckBig className="size-4" /> : <CircleAlert className="size-4" />}
              <AlertTitle>{t(`settings.customModels.discovery.${discoveryStateTranslationSegment(probeResult.status.state)}`)}</AlertTitle>
              <AlertDescription>{probeResult.status.message || t("settings.customProviders.validationResultDescription")}</AlertDescription>
            </Alert>
            <div className="grid gap-3 rounded-xl border p-4 sm:grid-cols-2">
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationModelCount")}</p><p className="font-medium">{probeResult.status.model_count}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationPageCount")}</p><p className="font-medium">{probeResult.catalog?.page_count ?? 0}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationCredential")}</p><p className="truncate font-medium">{validationConnection?.display_name ?? credentialDisplayName.trim()}</p></div>
              <div><p className="text-xs text-muted-foreground">{t("settings.customProviders.validationObservedAt")}</p><p className="font-medium">{formatCustomProviderObservationTime(probeResult.status.observed_at_ms)}</p></div>
            </div>
            {probeResult.catalog && probeResult.catalog.models.length > 0 ? (
              <CustomProviderProbeCatalogPreview result={probeResult} />
            ) : null}
            {probeResult.manualFallbackAvailable ? <p className="text-sm text-muted-foreground">{t("settings.customProviders.manualFallbackHint")}</p> : null}
            {probeResult.pendingSaveAvailable ? <p className="text-sm text-muted-foreground">{t("settings.customProviders.pendingSaveHint")}</p> : null}
          </div>
        ) : null}

        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        <DialogFooter>
          {step === "connection" ? <Button disabled={probing || saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button> : <Button disabled={probing || saving} onClick={() => setStep(step === "result" ? "credential" : "connection")} type="button" variant="outline">{t("settings.customProviders.back")}</Button>}
          {step === "connection" && props.provider && !providerChanged ? <Button disabled type="button">{t("memory.common.save")}</Button> : null}
          {step === "connection" && providerChanged && (directSaveOnly || credentialFreeEdit) ? <Button disabled={saving} onClick={credentialFreeEdit ? saveCredentialFreeEdit : () => void save()} type="button">{saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t("memory.common.save")}</Button> : null}
          {step === "connection" && (!props.provider || (providerChanged && !directSaveOnly && !credentialFreeEdit)) ? <Button onClick={continueToCredential} type="button">{t("settings.customProviders.next")}</Button> : null}
          {step === "credential" ? <Button disabled={probing} onClick={() => void probe()} type="button">{probing ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t(probing ? "settings.customProviders.probing" : "settings.customProviders.probe")}</Button> : null}
          {step === "result" && acceptedWithoutOverride ? <Button disabled={saving} onClick={() => void save()} type="button">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.saveVerified")}</Button> : null}
          {step === "result" && probeResult?.manualFallbackAvailable ? <Button disabled={saving} onClick={() => setPendingConfirmation("manual")} type="button">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.saveManual")}</Button> : null}
          {step === "result" && probeResult?.pendingSaveAvailable ? <Button disabled={saving} onClick={() => setPendingConfirmation("pending")} type="button" variant="secondary">{t(saving ? "settings.customProviders.saving" : "settings.customProviders.savePending")}</Button> : null}
        </DialogFooter>
        <AlertDialog
          onOpenChange={(open) => {
            if (!open && !saving) setPendingConfirmation(null)
          }}
          open={pendingConfirmation !== null}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("settings.customProviders.pendingConfirmTitle")}</AlertDialogTitle>
              <AlertDialogDescription>{t("settings.customProviders.pendingConfirmDescription")}</AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>{t("memory.common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                disabled={saving || pendingConfirmation === null}
                onClick={() => {
                  if (pendingConfirmation) void save(pendingConfirmation)
                }}
              >
                {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
                {t("settings.customProviders.confirmPending")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        <AlertDialog
          onOpenChange={(open) => {
            if (!open && !saving) setProtocolOverrideConfirmation(null)
          }}
          open={protocolOverrideConfirmation !== null}
        >
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t("settings.customProviders.protocolOverridesConfirmTitle")}</AlertDialogTitle>
              <AlertDialogDescription>
                {t("settings.customProviders.protocolOverridesConfirmDescription")
                  .replace("{count}", String(inactiveProtocolOverrideCount))
                  .replace("{protocol}", removedProtocols
                    .map((removed) => protocolDisplayName(customProtocolProfileId(removed)))
                    .join(", ") || t("settings.customProviders.protocol"))}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel disabled={saving}>{t("memory.common.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                disabled={saving || protocolOverrideConfirmation === null}
                onClick={() => {
                  const acceptance = protocolOverrideConfirmation === "ready"
                    ? undefined
                    : protocolOverrideConfirmation ?? undefined
                  void save(acceptance, true)
                }}
              >
                {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
                {t("settings.customProviders.protocolOverridesConfirmAction")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  )
}

// CustomProviderProbeCatalogPreview confirms discovered model identities before persistence.
// CustomProviderProbeCatalogPreview 在持久化前确认已发现的模型身份。
function CustomProviderProbeCatalogPreview(props: { result: PublicAiCustomProviderProbeResult }) {
  const { t } = useI18n()
  // models is the exact credential-scoped probe catalog and is never a persisted local copy.
  // models 是精确凭据作用域探测目录，绝不是持久化的本地副本。
  const models = props.result.catalog?.models ?? []
  return (
    <section className="overflow-hidden rounded-xl border">
      <div className="flex items-center justify-between gap-3 border-b bg-muted/30 px-4 py-2.5">
        <div className="min-w-0">
          <h3 className="text-sm font-medium">{t("settings.customProviders.validationModelsTitle")}</h3>
          <p className="truncate font-mono text-[11px] text-muted-foreground">
            {props.result.catalog?.discovery_schema_revision}
          </p>
        </div>
        <Badge variant="secondary">{models.length}</Badge>
      </div>
      <ScrollArea className="h-52">
        <div className="divide-y">
          {models.map((model) => (
            <div className="flex items-center justify-between gap-3 px-4 py-2.5" key={model.id}>
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">{model.display_name}</p>
                <p className="truncate font-mono text-xs text-muted-foreground">{model.id}</p>
              </div>
              <Badge variant={modelEvidenceCompleteness(model.capability_evidence) === 4 ? "secondary" : "outline"}>
                {t("settings.customProviders.validationEvidence")
                  .replace("{known}", String(modelEvidenceCompleteness(model.capability_evidence)))
                  .replace("{total}", "4")}
              </Badge>
            </div>
          ))}
        </div>
      </ScrollArea>
      <p className="border-t bg-muted/20 px-4 py-2 text-xs text-muted-foreground">
        {t("settings.customProviders.validationModelsHint")}
      </p>
    </section>
  )
}

// modelEvidenceCompleteness counts only explicitly declared capability dimensions.
// modelEvidenceCompleteness 仅统计上游明确声明的能力维度。
function modelEvidenceCompleteness(evidence: PublicAiModelCapabilityEvidence | undefined): number {
  if (!evidence) {
    return 0
  }
  return [
    evidence.context_limit_declared,
    evidence.output_limit_declared,
    evidence.input_modalities_declared,
    evidence.output_modalities_declared,
  ].filter(Boolean).length
}

// formatCustomProviderObservationTime renders a stable local timestamp for one probe.
// formatCustomProviderObservationTime 为一次探测渲染稳定的本地时间戳。
function formatCustomProviderObservationTime(observedAtMs: number): string {
  return observedAtMs > 0 ? new Date(observedAtMs).toLocaleString() : "—"
}

// CustomProviderConnectionDraft contains fields validated before credential collection.
// CustomProviderConnectionDraft 包含收集凭据前需要校验的字段。
interface CustomProviderConnectionDraft {
  baseUrl: string
  displayName: string
  modelDiscoveryMode: PublicAiCustomProviderModelDiscoveryMode
  modelDiscoveryPath: string
}

// validateCustomProviderConnectionStep returns the exact localized error key for invalid endpoint input.
// validateCustomProviderConnectionStep 为无效入口输入返回精确的本地化错误键。
//
// Parameters
// 参数
// `draft` is the provider identity, base URL, and model-discovery selection entered by the user.
// `draft` 是用户填写的供应商身份、基础 URL 与模型发现选择。
//
// Returns
// 返回值
// The corresponding translation key, or `null` when the first step is valid.
// 返回对应翻译键；第一步有效时返回 `null`。
function validateCustomProviderConnectionStep(draft: CustomProviderConnectionDraft): TranslationKey | null {
  if (!draft.displayName.trim()) {
    return "settings.customProviders.displayNameRequired"
  }
  try {
    const parsed = new URL(draft.baseUrl.trim())
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
      return "settings.customProviders.baseUrlInvalid"
    }
  } catch {
    return "settings.customProviders.baseUrlInvalid"
  }
  if (draft.modelDiscoveryMode === "custom_path") {
    const path = draft.modelDiscoveryPath.trim()
    if (!path.startsWith("/") || path.includes("://") || path.split("/").includes("..")) {
      return "settings.customProviders.discoveryPathInvalid"
    }
  }
  return null
}

// customProviderDiscoveryMode reconstructs the persisted discovery choice from one exact service entry.
// customProviderDiscoveryMode 从一个精确服务入口重建持久化的发现选择。
//
// Parameters
// 参数
// `entry` is the single custom-provider service entry, when editing an existing definition.
// `entry` 是编辑现有定义时的唯一自定义供应商服务入口。
//
// Returns
// 返回值
// The user-facing discovery mode represented by the runtime descriptor.
// 返回运行时描述所表示的用户发现模式。
function customProviderDiscoveryMode(entry: PublicAiServiceEntry | undefined): PublicAiCustomProviderModelDiscoveryMode {
  if (entry?.model_discovery === "manual") {
    return "manual"
  }
  if (entry?.protocol_routes[0]?.model_discovery_path) {
    return "custom_path"
  }
  return "standard"
}

// customProviderAuthentication maps one persisted entry to the editable authentication contract.
// customProviderAuthentication 将一个持久化入口映射为可编辑鉴权契约。
//
// Parameters
// 参数
// `entry` is the exact persisted custom-provider service entry.
// `entry` 是精确的持久化自定义供应商服务入口。
//
// Returns
// 返回值
// The single supported browser authentication choice.
// 返回唯一受支持的浏览器鉴权选择。
function customProviderAuthentication(
  entry: PublicAiServiceEntry | undefined,
): "bearer_authorization" | "google_api_key" | "x_api_key" {
  if (entry?.authentication === "google_api_key") {
    return "google_api_key"
  }
  return entry?.authentication === "x_api_key" ? "x_api_key" : "bearer_authorization"
}

// customProviderEndpointPreview joins the exact protocol path to the user-entered base URL.
// customProviderEndpointPreview 将精确协议路径拼接到用户输入的基础 URL。
//
// Parameters
// 参数
// `baseUrl` is the unpersisted service origin and `protocol` selects one fixed route path.
// `baseUrl` 是尚未持久化的服务源，`protocol` 选择一条固定路由路径。
//
// Returns
// 返回值
// A display-only endpoint preview, or an empty string when no base URL exists.
// 返回仅用于展示的端点预览；没有基础 URL 时返回空字符串。
function customProviderEndpointPreview(baseUrl: string, protocol: PublicAiCustomProviderProtocol): string {
  if (protocol === "gemini_generate_content") {
    return joinCustomProviderUrl(baseUrl, "/models/{model}:streamGenerateContent?alt=sse")
  }
  const path = protocol === "anthropic_messages"
    ? "/messages"
    : protocol === "open_ai_chat"
      ? "/chat/completions"
      : "/responses"
  return joinCustomProviderUrl(baseUrl, path)
}

// customProviderDiscoveryEndpointPreview returns the selected model-catalog endpoint preview.
// customProviderDiscoveryEndpointPreview 返回所选模型目录端点预览。
//
// Parameters
// 参数
// `baseUrl` is the service origin, `mode` is the discovery strategy, and `customPath` is the optional declared path.
// `baseUrl` 是服务源，`mode` 是发现策略，`customPath` 是可选的声明路径。
//
// Returns
// 返回值
// A display-only URL, or an empty string for manual discovery.
// 返回仅用于展示的 URL；手工发现时返回空字符串。
function customProviderDiscoveryEndpointPreview(
  baseUrl: string,
  mode: PublicAiCustomProviderModelDiscoveryMode,
  customPath: string,
): string {
  if (mode === "manual") {
    return ""
  }
  return joinCustomProviderUrl(baseUrl, mode === "custom_path" ? customPath : "/models")
}

// joinCustomProviderUrl normalizes one display-only base/path pair without changing persisted values.
// joinCustomProviderUrl 规范化一组仅用于展示的基础 URL 与路径且不修改持久化值。
//
// Parameters
// 参数
// `baseUrl` is the raw base and `path` is one same-origin absolute endpoint path.
// `baseUrl` 是原始基础 URL，`path` 是一个同源绝对端点路径。
//
// Returns
// 返回值
// The joined preview URL, or an empty string when either side is incomplete.
// 返回拼接后的预览 URL；任一侧不完整时返回空字符串。
function joinCustomProviderUrl(baseUrl: string, path: string): string {
  const normalizedBase = baseUrl.trim().replace(/\/+$/, "")
  const normalizedPath = path.trim()
  if (!normalizedBase || !normalizedPath) {
    return ""
  }
  return `${normalizedBase}/${normalizedPath.replace(/^\/+/, "")}`
}

// discoveryStateTranslationSegment maps the closed backend state to its translation-key suffix.
// discoveryStateTranslationSegment 将封闭后端状态映射为翻译键后缀。
//
// Parameters
// 参数
// `state` is one serialized discovery state returned by the service.
// `state` 是服务返回的一项序列化发现状态。
//
// Returns
// 返回值
// The exact suffix registered in the i18n catalog.
// 返回 i18n 目录中注册的精确后缀。
function discoveryStateTranslationSegment(
  state: PublicAiCustomProviderProbeResult["status"]["state"],
): "notChecked" | "ready" | "empty" | "unsupported" | "authenticationFailed" | "rateLimited" | "temporarilyUnavailable" | "invalidResponse" | "stale" | "failed" {
  return discoveryStateTranslationSegments[state]
}

// ProviderSummaryTooltip renders one compact count whose hover card lists exact values.
// ProviderSummaryTooltip 渲染一个紧凑数量摘要，并在悬停卡片中列出精确值。
//
// Parameters
// 参数
// `label` is the compact summary, `title` names the collection, and `items` are exact entries.
// `label` 是紧凑摘要，`title` 是集合名称，`items` 是精确条目。
//
// Returns
// 返回值
// A keyboard-focusable, hover-persistent tooltip trigger.
// 返回可通过键盘聚焦且允许鼠标驻留的 Tooltip 触发器。
function ProviderSummaryTooltip(props: {
  items: string[]
  label: string
  title: string
}) {
  return (
    <Tooltip delayDuration={500}>
      <TooltipTrigger asChild>
        <button
          className="inline-flex h-6 items-center rounded-full border border-border px-2.5 text-xs font-medium transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          type="button"
        >
          {props.label}
        </button>
      </TooltipTrigger>
      <TooltipContent className="block max-w-md space-y-2 px-3 py-2" side="bottom" sideOffset={6}>
        <p className="font-medium">{props.title}</p>
        <div className="flex max-w-sm flex-wrap gap-1.5">
          {props.items.map((item) => (
            <span className="rounded border border-background/25 px-1.5 py-0.5" key={item}>
              {item}
            </span>
          ))}
        </div>
      </TooltipContent>
    </Tooltip>
  )
}

// uniqueProtocolNames returns the stable unique protocol labels declared by one provider.
// uniqueProtocolNames 返回一个供应商声明的稳定唯一协议标签。
function uniqueProtocolNames(provider: PublicAiProviderDescriptor): string[] {
  return Array.from(new Set(provider.service_entries.flatMap(
    (entry) => entry.protocol_routes.map((route) => protocolDisplayName(route.protocol_profile_id)),
  )))
}

// protocolDisplayName returns the user-visible label for one exact profile.
// protocolDisplayName 返回一个精确 Profile 的用户可见标签。
function protocolDisplayName(profileId: string): string {
  return protocolDisplayNames[profileId] ?? profileId
}

// errorMessage normalizes service failures for inline rendering.
// errorMessage 归一化服务失败以便行内渲染。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}

// customProtocolOptions is the complete browser-editable compatibility protocol set.
// customProtocolOptions 是浏览器可编辑的完整兼容协议集合。
const customProtocolOptions: Array<{ id: PublicAiCustomProviderProtocol; label: string }> = [
  { id: "anthropic_messages", label: "Anthropic Messages" },
  { id: "gemini_generate_content", label: "Gemini Generate Content" },
  { id: "open_ai_chat", label: "OpenAI Chat Completions" },
  { id: "open_ai_responses", label: "OpenAI Responses" },
  { id: "open_ai_codex_responses", label: "Codex Responses (full compatibility)" },
]

// customProviderStepLabelKeys keeps step labels inside the closed translation-key contract.
// customProviderStepLabelKeys 将步骤标签保持在封闭翻译键契约内。
const customProviderStepLabelKeys = {
  connection: "settings.customProviders.step.connection",
  credential: "settings.customProviders.step.credential",
  result: "settings.customProviders.step.result",
} as const

// customProviderDiscoveryModeLabelKeys maps user discovery choices to localized labels.
// customProviderDiscoveryModeLabelKeys 将用户发现选择映射为本地化标签。
const customProviderDiscoveryModeLabelKeys = {
  standard: "settings.customProviders.discovery.standard",
  custom_path: "settings.customProviders.discovery.custom_path",
  manual: "settings.customProviders.discovery.manual",
} as const

// discoveryStateTranslationSegments maps serialized states to camel-cased i18n suffixes.
// discoveryStateTranslationSegments 将序列化状态映射为驼峰形式的 i18n 后缀。
const discoveryStateTranslationSegments = {
  not_checked: "notChecked",
  ready: "ready",
  empty: "empty",
  unsupported: "unsupported",
  authentication_failed: "authenticationFailed",
  rate_limited: "rateLimited",
  temporarily_unavailable: "temporarilyUnavailable",
  invalid_response: "invalidResponse",
  stale: "stale",
  failed: "failed",
} as const

// protocolDisplayNames is the complete user-visible label map for registered first-batch profiles.
// protocolDisplayNames 是首批已注册 Profile 的完整用户可见名称映射。
const protocolDisplayNames: Readonly<Record<string, string>> = {
  "anthropic-messages.v1": "Anthropic Messages",
  "claude-code-messages.v1": "Claude Code",
  "gemini-interactions.v1": "Gemini Interactions",
  "google.code_assist.generate_content.v1": "Google Code Assist",
  "google.gemini.generate_content": "Gemini Generate Content",
  "openai-chat-completions.v1": "OpenAI Chat Completions",
  "openai-codex-responses.v1": "OpenAI Codex",
  "openai-responses.v1": "OpenAI Responses",
}
