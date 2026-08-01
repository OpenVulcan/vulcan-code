import * as React from "react"
import {
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
import { localizedProviderDescription } from "@/components/features/settings/provider-presentation"
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
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
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
import {
  loadPublicAiSettings,
  removePublicAiCustomProvider,
  savePublicAiCustomProvider,
} from "@/service/settings-actions-service"
import type {
  PublicAiCustomProviderProtocol,
  PublicAiProviderDescriptor,
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

  // removeProvider removes one custom provider only when no credential references it.
  // removeProvider 仅在没有凭据引用时移除一个自定义供应商。
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

  // systemProviders contains the immutable built-in provider definitions.
  // systemProviders 包含不可变的内置供应商定义。
  const systemProviders = snapshot?.providers.filter((provider) => provider.kind === "system") ?? []
  // customProviders contains the user-managed provider definitions.
  // customProviders 包含用户管理的供应商定义。
  const customProviders = snapshot?.providers.filter((provider) => provider.kind === "custom") ?? []

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
        onOpenChange={setDialogOpen}
        onSaved={handleProviderSaved}
        open={dialogOpen}
        provider={editingProvider}
      />
      <CustomProviderModelsDialog
        onOpenChange={setModelsDialogOpen}
        onSaved={setSnapshot}
        open={modelsDialogOpen}
        provider={modelsProvider}
        snapshot={snapshot}
      />
      <AlertDialog onOpenChange={(open) => !open && setProviderToRemove(null)} open={providerToRemove !== null}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customProviders.removeTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.customProviders.removeDescription").replace("{name}", providerToRemove?.display_name ?? "")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={providerToRemove ? busyProviderId === providerToRemove.id : true}
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

// CustomProviderDialog edits the exact public custom-provider contract.
// CustomProviderDialog 编辑精确的公开自定义供应商契约。
function CustomProviderDialog(props: {
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  provider: PublicAiProviderDescriptor | null
}) {
  const [displayName, setDisplayName] = React.useState("")
  const [baseUrl, setBaseUrl] = React.useState("")
  const [protocol, setProtocol] = React.useState<PublicAiCustomProviderProtocol>("open_ai_responses")
  const [authentication, setAuthentication] = React.useState<"bearer_authorization" | "x_api_key">("bearer_authorization")
  const [credentialDisplayName, setCredentialDisplayName] = React.useState("")
  const [secret, setSecret] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const entry = props.provider?.service_entries[0]
    setDisplayName(props.provider?.display_name ?? "")
    setBaseUrl(entry?.base_url ?? "")
    setAuthentication(entry?.authentication === "x_api_key" ? "x_api_key" : "bearer_authorization")
    setProtocol(customProtocolFromProfile(entry?.protocol_routes[0]?.protocol_profile_id ?? "") ?? "open_ai_responses")
    setCredentialDisplayName(props.provider?.display_name ?? "")
    setSecret("")
    setError(null)
  }, [props.open, props.provider])

  // save requires authenticated model discovery before persisting the descriptor.
  // save 要求先完成鉴权模型发现，再持久化描述符。
  const save = React.useCallback(async () => {
    if (!displayName.trim() || !baseUrl.trim() || !credentialDisplayName.trim() || !secret.trim()) {
      setError("请填写显示名称、基础 URL、凭据名称和有效 API Key。")
      return
    }
    setSaving(true)
    setError(null)
    try {
      const nextSnapshot = await savePublicAiCustomProvider({
        providerId: props.provider?.id,
        displayName: displayName.trim(),
        baseUrl: baseUrl.trim(),
        protocol,
        authentication,
        credentialDisplayName: credentialDisplayName.trim(),
        secret: secret.trim(),
      })
      props.onSaved(nextSnapshot)
      props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [authentication, baseUrl, credentialDisplayName, displayName, props, protocol, secret])

  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="sm:max-w-[560px]">
        <DialogHeader>
          <DialogTitle>{props.provider ? "编辑自定义供应商" : "添加自定义供应商"}</DialogTitle>
          <DialogDescription>
            一个基础 URL 只绑定一种协议。保存前会使用 API Key 拉取模型目录；新增时验证成功的 Key 将作为首个凭据保存。
          </DialogDescription>
        </DialogHeader>
        <label className="settings-form-field">
          <span>显示名称</span>
          <Input onChange={(event) => setDisplayName(event.target.value)} value={displayName} />
        </label>
        <label className="settings-form-field">
          <span>基础 URL</span>
          <Input onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://example.com/v1" value={baseUrl} />
        </label>
        <label className="settings-form-field">
          <span>鉴权请求头</span>
          <Select onValueChange={(value) => setAuthentication(value as "bearer_authorization" | "x_api_key")} value={authentication}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="bearer_authorization">Authorization: Bearer</SelectItem>
              <SelectItem value="x_api_key">x-api-key</SelectItem>
            </SelectContent>
          </Select>
        </label>
        <label className="settings-form-field">
          <span>调用协议</span>
          <Select onValueChange={(value) => setProtocol(value as PublicAiCustomProviderProtocol)} value={protocol}>
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              {customProtocolOptions.map((option) => (
                <SelectItem key={option.id} value={option.id}>{option.label}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </label>
        <label className="settings-form-field">
          <span>{props.provider ? "验证凭据名称" : "首个凭据名称"}</span>
          <Input
            onChange={(event) => setCredentialDisplayName(event.target.value)}
            value={credentialDisplayName}
          />
        </label>
        <label className="settings-form-field">
          <span>API Key</span>
          <Input
            autoComplete="off"
            onChange={(event) => setSecret(event.target.value)}
            placeholder={props.provider ? "仅用于验证本次配置，不会新增凭据" : "保存前必须通过模型目录验证"}
            type="password"
            value={secret}
          />
        </label>
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        <DialogFooter>
          <Button onClick={() => props.onOpenChange(false)} type="button" variant="outline">
            取消
          </Button>
          <Button disabled={saving} onClick={() => void save()} type="button">
            {saving ? "保存中…" : "保存供应商"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
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

// customProtocolFromProfile maps one exact runtime profile to the custom-provider contract.
// customProtocolFromProfile 将一个精确运行时 Profile 映射到自定义供应商契约。
function customProtocolFromProfile(profileId: string): PublicAiCustomProviderProtocol | null {
  if (profileId === "anthropic-messages.v1") {
    return "anthropic_messages"
  }
  if (profileId === "openai-chat-completions.v1") {
    return "open_ai_chat"
  }
  if (profileId === "openai-responses.v1") {
    return "open_ai_responses"
  }
  return null
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
  { id: "open_ai_chat", label: "OpenAI Chat Completions" },
  { id: "open_ai_responses", label: "OpenAI Responses" },
]

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
