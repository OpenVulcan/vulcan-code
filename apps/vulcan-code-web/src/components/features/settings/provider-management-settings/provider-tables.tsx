// Custom and system provider tables with the provider priority editor.
// 自定义与系统供应商表格，以及供应商优先级编辑器。
import * as React from "react"
import { KeyRound, LoaderCircle, ListTree, LockKeyhole, Pencil, Plus, Search, ServerCog, Trash2 } from "lucide-react"
import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { SettingsEmptyState } from "@/components/features/settings/settings-empty-state"
import { orderProvidersByPriority } from "@/components/features/settings/provider-ordering"
import { localizedProviderDescription, localizedProviderName } from "@/components/features/settings/provider-presentation"
import { StreamPlaceholder } from "@/components/ui/stream-placeholder"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import type { PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"
import { protocolDisplayName } from "./protocol-display-name"

// CustomProviderTable renders the Model Core custom-provider table structure.
// CustomProviderTable 渲染 Model Core 的自定义供应商表格结构。
export function CustomProviderTable(props: {
  busyProviderId: string | null
  connections: PublicAiSettingsSnapshot["connections"]
  customModels: NonNullable<PublicAiSettingsSnapshot["customModels"]>
  loading: boolean
  onAddCredential: (providerId: string) => void
  onCreate: () => void
  onEdit: (provider: PublicAiProviderDescriptor) => void
  onManageModels: (provider: PublicAiProviderDescriptor) => void
  onRemove: (provider: PublicAiProviderDescriptor) => void
  onSavePriority: (providerId: string, priority: number) => void
  priorities: Readonly<Record<string, number>>
  providers: PublicAiProviderDescriptor[]
}) {
  const { t } = useI18n()
  // orderedProviders applies the shared high-to-low priority order without mutating the snapshot.
  // orderedProviders 应用共享的从高到低优先级顺序，且不修改快照。
  const orderedProviders = React.useMemo(
    () => orderProvidersByPriority(props.providers, props.priorities),
    [props.priorities, props.providers],
  )
  // search retains this category's local filter across directory switches.
  // search 在目录切换时保留本分类的局部筛选。
  const [search, setSearch] = React.useState("")
  // visibleProviders filters registered identities without changing priority or credentials.
  // visibleProviders 筛选已注册身份，不改变优先级或凭据。
  const visibleProviders = orderedProviders.filter((provider) => (
    `${provider.id} ${provider.display_name}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())
  ))
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <ServerCog className="size-4" />
          <h3 className="font-medium">{t("settings.providers.customTitle")}</h3>
          <Badge variant="secondary">{props.providers.length}</Badge>
        </div>
        <Button
          type="button"
          size="sm"
          className="inline-flex items-center gap-1.5 leading-none"
          disabled={props.busyProviderId !== null}
          onClick={props.onCreate}
        >
          <Plus className="size-4" />
          <span className="leading-none">{t("settings.providers.addProvider")}</span>
        </Button>
      </div>

      <Input aria-label={t("settings.modelExperience.searchCustom")} placeholder={t("settings.modelExperience.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} />
      {!props.loading && props.providers.length > 0 && visibleProviders.length === 0 ? <SettingsEmptyState icon={<Search />} title={t("settings.modelExperience.noProviders")} /> : null}
      {props.loading ? (
        <StreamPlaceholder className="rounded-xl border p-1.5" fill label={t("settings.providers.loading")} shape="rows" count={4} />
      ) : props.providers.length === 0 ? (
        <SettingsEmptyState
          action={(
            <Button disabled={props.busyProviderId !== null} onClick={props.onCreate} size="sm" type="button">
              <Plus className="size-4" />
              {t("settings.providers.addProvider")}
            </Button>
          )}
          icon={<ServerCog />}
          title={t("settings.providers.emptyCustom")}
        />
      ) : visibleProviders.length === 0 ? null : (
        <div className="overflow-hidden rounded-xl border">
          <Table className="provider-responsive-table">
            <TableHeader className="bg-muted/40">
              <TableRow>
                <TableHead className="w-[18rem]">{t("settings.providers.table.provider")}</TableHead>
                <TableHead>{t("settings.providers.table.endpoint")}</TableHead>
                <TableHead className="w-32">{t("settings.providers.table.resources")}</TableHead>
                <TableHead className="w-28 text-center">{t("settings.providers.table.status")}</TableHead>
                <TableHead className="w-36">{t("settings.providers.table.priority")}</TableHead>
                <TableHead className="w-52 text-right">{t("settings.providers.table.actions")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visibleProviders.map((provider) => {
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
                    <TableCell data-label={t("settings.providers.table.provider")} className="whitespace-normal py-3 align-top">
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
                    <TableCell data-label={t("settings.providers.table.endpoint")} className="whitespace-normal py-3 align-top">
                      <div className="font-medium">
                        {protocolNames.length > 0 ? protocolNames.join(" · ") : t("settings.providers.table.protocolUnavailable")}
                      </div>
                      <div className="mt-1 space-y-1 text-xs text-muted-foreground">
                        {provider.service_entries.map((entry) => (
                          <div className="break-all font-mono" key={entry.id}>
                            {entry.base_url}
                          </div>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.resources")} className="whitespace-normal py-3 align-top text-sm text-muted-foreground">
                      <div>{t("settings.customProviders.modelCount").replace("{count}", String(modelCount))}</div>
                        <div className="mt-1">{t("settings.providers.credentialCount").replace("{count}", String(credentialCount))}</div>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.status")} className="py-3 text-center align-top">
                      <Badge variant={credentialCount > 0 ? "default" : "secondary"}>
                        {credentialCount > 0 ? t("settings.providers.configured") : t("settings.providers.notConfigured")}
                      </Badge>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.priority")} className="py-3 align-top">
                      <ProviderPriorityEditor
                        busy={props.busyProviderId !== null}
                        onSave={(priority) => props.onSavePriority(provider.id, priority)}
                        value={props.priorities[provider.id] ?? 0}
                      />
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.actions")} className="py-3 text-right align-top">
                      <div className="flex justify-end gap-1">
                        <Button
                          aria-label={t("settings.providers.addCredentialFor").replace("{provider}", provider.display_name)}
                          disabled={props.busyProviderId !== null}
                          onClick={() => props.onAddCredential(provider.id)}
                          size="sm"
                          type="button"
                          variant="outline"
                        >
                          <KeyRound className="mr-1 size-3.5" />
                          {t("settings.providerOnboarding.addCredential")}
                        </Button>
                        <Button
                          aria-label={t("settings.customProviders.manageModelsFor").replace("{provider}", provider.display_name)}
                          disabled={props.busyProviderId !== null}
                          onClick={() => props.onManageModels(provider)}
                          size="icon-sm"
                          title={t("settings.customProviders.manageModels")}
                          type="button"
                          variant="outline"
                        >
                          <ListTree className="size-4" />
                        </Button>
                        <Button
                          aria-label={t("settings.providers.editFor").replace("{provider}", provider.display_name)}
                          disabled={props.busyProviderId !== null}
                          onClick={() => props.onEdit(provider)}
                          size="icon-sm"
                          type="button"
                          variant="outline"
                        >
                          <Pencil className="size-4" />
                        </Button>
                        <Button
                          aria-label={t("settings.providers.removeFor").replace("{provider}", provider.display_name)}
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
export function SystemProviderTable(props: {
  busyProviderId: string | null
  connections: PublicAiSettingsSnapshot["connections"]
  loading: boolean
  onAddCredential: (providerId: string) => void
  onManageModels?: (provider: PublicAiProviderDescriptor) => void
  onSavePriority: (providerId: string, priority: number) => void
  priorities: Readonly<Record<string, number>>
  providers: PublicAiProviderDescriptor[]
  variant: "system" | "functional"
}) {
  const { t } = useI18n()
  // orderedProviders applies priority first and preserves the previous promotional tie-break.
  // orderedProviders 优先应用优先级，并保留原有推广同级排序。
  const orderedProviders = React.useMemo(
    () => orderProvidersByPriority(props.providers, props.priorities),
    [props.priorities, props.providers],
  )
  // search retains the system or functional category filter while navigating.
  // search 在导航切换时保留系统或功能分类筛选。
  const [search, setSearch] = React.useState("")
  // visibleProviders searches both registered and localized names without mutating catalog state.
  // visibleProviders 同时搜索注册名称和本地化名称，不修改目录状态。
  const visibleProviders = orderedProviders.filter((provider) => (
    `${provider.id} ${provider.display_name} ${localizedProviderName(provider.id, provider.display_name, t)}`
      .toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())
  ))
  return (
    <section className="space-y-3">
      <Input aria-label={t("settings.modelExperience.searchProvider")} placeholder={t("settings.modelExperience.searchPlaceholder")} value={search} onChange={(event) => setSearch(event.target.value)} />
      {!props.loading && visibleProviders.length === 0 ? <SettingsEmptyState icon={search.trim() ? <Search /> : props.variant === "system" ? <LockKeyhole /> : <ServerCog />} title={t("settings.modelExperience.noProviders")} /> : null}
      {props.loading ? (
        <StreamPlaceholder className="rounded-xl border p-1.5" fill label={t("settings.providers.loading")} shape="rows" count={4} />
      ) : visibleProviders.length === 0 ? null : (
        <div className="overflow-hidden rounded-xl border">
          <Table className="provider-responsive-table">
            <TableHeader className="bg-muted/40">
              <TableRow>
                <TableHead className="w-[18rem]">{t("settings.providers.table.provider")}</TableHead>
                <TableHead>{t("settings.providers.table.details")}</TableHead>
                <TableHead className="w-32 text-right">{t("settings.providers.table.configurationStatus")}</TableHead>
                <TableHead className="w-36">{t("settings.providers.table.priority")}</TableHead>
                <TableHead className="w-28 text-right">{t("settings.providers.table.operation")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {visibleProviders.map((provider) => {
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
                // providerName localizes registered product names without touching custom providers.
                // providerName 本地化已注册产品名称，同时不改写自定义供应商。
                const providerName = localizedProviderName(provider.id, provider.display_name, t)
                return (
                  <TableRow key={provider.id}>
                    <TableCell data-label={t("settings.providers.table.provider")} className="whitespace-normal py-3 align-middle">
                      <div className="flex items-center gap-3">
                        <ProviderIcon category={provider.category} className="size-[26px]" providerId={provider.id} />
                        <div className="min-w-0">
                          <span className="font-medium">{providerName}</span>
                          <div className="mt-1 truncate font-mono text-xs text-muted-foreground">
                            {provider.id}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.details")} className="py-3 align-middle">
                      {/* Portaled details preserve row height and support both pointer hover and keyboard focus. */}
                      {/* 门户提示保持行高稳定，同时支持鼠标悬停与键盘聚焦。 */}
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button className="text-muted-foreground" size="sm" type="button" variant="ghost">
                            {t("settings.modelExperience.providerDetails")}
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent className="block w-80 max-w-[calc(100vw-2rem)] space-y-3 p-3" side="bottom" sideOffset={6}>
                          <div className="space-y-1">
                            <p className="font-medium">{providerName}</p>
                            <p>{providerDescription}</p>
                          </div>
                          <div className="space-y-1">
                            <p className="font-medium">{t("settings.providerSummary.serviceEntries").replace("{count}", String(provider.service_entries.length))}</p>
                            <ul className="space-y-1">{provider.service_entries.map((entry) => <li key={entry.id}>{entry.display_name}</li>)}</ul>
                          </div>
                          {protocolNames.length > 0 ? <div className="space-y-1">
                            <p className="font-medium">{t("settings.providerSummary.protocols").replace("{count}", String(protocolNames.length))}</p>
                            <ul className="space-y-1">{protocolNames.map((name) => <li key={name}>{name}</li>)}</ul>
                          </div> : null}
                        </TooltipContent>
                      </Tooltip>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.configurationStatus")} className="py-3 text-right align-middle">
                      <Badge variant={credentialCount > 0 ? "default" : "secondary"}>
                        {credentialCount > 0 ? t("settings.providers.credentialCount").replace("{count}", String(credentialCount)) : t("settings.providers.notConfigured")}
                      </Badge>
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.priority")} className="py-3 align-middle">
                      <ProviderPriorityEditor
                        busy={props.busyProviderId !== null}
                        onSave={(priority) => props.onSavePriority(provider.id, priority)}
                        value={props.priorities[provider.id] ?? 0}
                      />
                    </TableCell>
                    <TableCell data-label={t("settings.providers.table.operation")} className="py-3 text-right align-middle">
                      <div className="flex justify-end gap-1">
                        {props.variant === "system" && props.onManageModels ? <Button
                          aria-label={t("settings.customProviders.manageModelsFor").replace("{provider}", providerName)}
                          disabled={props.busyProviderId !== null}
                          onClick={() => props.onManageModels?.(provider)}
                          size="icon-sm"
                          title={t("settings.customProviders.manageModels")}
                          type="button"
                          variant="outline"
                        ><ListTree className="size-4" /></Button> : null}
                        <Button
                          disabled={props.busyProviderId !== null}
                          onClick={() => props.onAddCredential(provider.id)}
                          size="sm"
                          type="button"
                          variant="outline"
                        >
                          <KeyRound className="mr-1 size-3.5" />
                          {t("settings.providerOnboarding.addCredential")}
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

// ProviderPriorityEditorProps defines one explicit provider-priority mutation control.
// ProviderPriorityEditorProps 定义一个显式供应商优先级变更控件。
interface ProviderPriorityEditorProps {
  // busy prevents edits while this provider mutation is running.
  // busy 在当前供应商变更运行期间阻止编辑。
  busy: boolean
  // onSave receives one validated signed 32-bit priority.
  // onSave 接收一个已验证的有符号 32 位优先级。
  onSave: (priority: number) => void
  // value is the current persisted priority including the inherited zero default.
  // value 是当前持久化优先级，包含继承的零默认值。
  value: number
}

// ProviderPriorityEditor edits and explicitly saves one signed 32-bit ordering value.
// ProviderPriorityEditor 编辑并显式保存一个有符号 32 位排序值。
function ProviderPriorityEditor(props: ProviderPriorityEditorProps) {
  const { t } = useI18n()
  // draft preserves incomplete numeric input until the user explicitly saves it.
  // draft 在用户显式保存前保留尚未完成的数字输入。
  const [draft, setDraft] = React.useState(String(props.value))
  React.useEffect(() => {
    setDraft(String(props.value))
  }, [props.value])
  // parsedPriority is the exact JavaScript numeric representation of the current draft.
  // parsedPriority 是当前草稿的精确 JavaScript 数字表示。
  const parsedPriority = Number(draft)
  // validPriority enforces the backend i32 contract before dispatch.
  // validPriority 在发送前强制执行后端 i32 契约。
  const validPriority = draft.trim().length > 0
    && Number.isInteger(parsedPriority)
    && parsedPriority >= -2_147_483_648
    && parsedPriority <= 2_147_483_647
  // changed reports whether a valid save would alter persisted state.
  // changed 报告有效保存是否会改变持久化状态。
  const changed = validPriority && parsedPriority !== props.value
  // save dispatches only one valid changed value.
  // save 仅发送一个有效且已变化的值。
  const save = React.useCallback(() => {
    if (changed) {
      props.onSave(parsedPriority)
    }
  }, [changed, parsedPriority, props])
  return (
    <div className="flex items-center gap-1">
      <Input
        aria-label={t("settings.providers.priorityAria")}
        className="h-8 w-20"
        disabled={props.busy}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault()
            save()
          }
        }}
        step={1}
        type="number"
        value={draft}
      />
      <Button
        disabled={props.busy || !changed}
        onClick={save}
        size="sm"
        type="button"
        variant="ghost"
      >
        {t("settings.providers.savePriority")}
      </Button>
    </div>
  )
}

// uniqueProtocolNames returns the stable unique protocol labels declared by one provider.
// uniqueProtocolNames 返回一个供应商声明的稳定唯一协议标签。
function uniqueProtocolNames(provider: PublicAiProviderDescriptor): string[] {
  return Array.from(new Set(provider.service_entries.flatMap(
    (entry) => entry.protocol_routes.map((route) => protocolDisplayName(route.protocol_profile_id)),
  )))
}
