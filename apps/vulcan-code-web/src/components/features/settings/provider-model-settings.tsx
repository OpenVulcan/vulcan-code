import * as React from "react"
import {
  Search,
  SlidersHorizontal,
} from "lucide-react"

import {
  filterProviderModels,
  type FilterableProviderModel,
} from "@/components/features/settings/provider-model-filter"
import type { SnapshotMutations } from "@/components/features/settings/model-services/use-snapshot-mutations"
import { modelModalityLabel, ProviderModelModalitySummary } from "@/components/features/settings/provider-model-modality-summary"
import { providerExtensionOutputModalities, providerModelModalities } from "@/components/features/settings/provider-model-modalities"
import {
  buildProviderModelInventoryGroups,
  filterProviderModelInventoryGroupsWithAvailableAccounts,
  type PublicProviderModelInventoryEntry,
  type PublicModelProtocolRoute,
} from "@/components/features/settings/provider-model-routes"
import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { isModelExtensionCapability } from "@/lib/provider-tool-capabilities"
import { PublicAiModelOverridesDialog } from "@/components/features/settings/public-ai-model-overrides-dialog"
import { PublicAiModelCapabilityDialog } from "@/components/features/settings/public-ai-model-capability-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import {
  savePublicAiConnectionModelEnabled,
  savePublicAiConnectionModelSetEnabled,
  savePublicAiInternalModelEnabled,
  savePublicAiInternalModelSetEnabled,
  savePublicAiMultimodalUnderstandingCandidate,
  savePublicAiModelEnabled,
  savePublicAiModelSetEnabled,
  savePublicAiProviderExtensionTargetSet,
  savePublicAiProviderToolCapabilityEnabled,
} from "@/service/settings-actions-service"
import type {
  PublicAiCodexModelBehavior,
  PublicAiConnection,
  PublicAiModel,
  PublicAiOperation,
  PublicAiProviderDescriptor,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSnapshot,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicProviderModelRow binds one rendered provider model to its routes and effective enabled state.
// PublicProviderModelRow 将一个渲染中的供应商模型绑定到其路由与有效启用状态。
interface PublicProviderModelRow extends FilterableProviderModel {
  // Provider that owns this model; the same model of another provider is a separate row.
  // 持有该模型的供应商；其他供应商的同名模型是独立的行。
  provider: PublicAiProviderDescriptor
  // Stable row identity across providers.
  // 跨供应商的稳定行标识。
  key: string
  // Complete account and signed-template facts backing this provider model.
  // 该供应商模型背后的完整账号与签名模板事实。
  inventory: PublicProviderModelInventoryEntry
  // Protocol-scoped routes exposing this model through configured accounts.
  // 通过已配置账号暴露此模型的协议作用域路由。
  routes: PublicModelProtocolRoute[]
}

// extensionModelTargets retains only exact signed, non-custom targets for one provider model and capability.
// extensionModelTargets 仅保留某供应商模型和能力的精确签名非自定义目标。
// Parameters: snapshot owns target facts and the remaining identifiers select one extension model; returns selectable targets.
// 参数：snapshot 持有目标事实，其余标识定位一个扩展模型；返回可选择的目标。
function extensionModelTargets(
  snapshot: PublicAiSettingsSnapshot,
  providerId: string,
  modelId: string,
  capability: PublicAiProviderToolCapabilityKind,
): PublicAiProviderToolTargetSnapshot[] {
  return snapshot.providerToolTargets.filter((target) => (
    target.target.provider_id === providerId
    && target.target.upstream_target_id === modelId
    && target.target.contract.capability === capability
    && target.manifestSourceLayer !== "custom"
  ))
}

// extensionCapabilityLabel resolves the localized action represented by one signed extension switch.
// extensionCapabilityLabel 解析签名扩展开关所代表操作的本地化名称。
// Parameters: capability is a closed extension kind and t is the locale resolver; returns its user-facing label.
// 参数：capability 是封闭扩展类型，t 是语言解析器；返回面向用户的标签。
function extensionCapabilityLabel(
  capability: PublicAiProviderToolCapabilityKind,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (capability) {
    case "image_generate": return t("settings.providerTools.imageGenerate")
    case "image_edit": return t("settings.providerTools.imageEdit")
    case "video_generate": return t("settings.providerTools.videoGenerate")
    case "video_edit": return t("settings.providerTools.videoEdit")
    case "video_extend": return t("settings.providerTools.videoExtend")
    case "speech_synthesize": return t("settings.providerTools.speechSynthesize")
    case "speech_transcribe": return t("settings.providerTools.speechTranscribe")
    case "music_generate": return t("settings.providerTools.musicGenerate")
    case "music_cover": return t("settings.providerTools.musicCover")
    default: return capability
  }
}

// rowConversationEligible follows declared operations, independently from the access preference.
// rowConversationEligible 遵循声明操作，与访问偏好相互独立。
function rowConversationEligible(row: PublicProviderModelRow): boolean {
  return row.routes.some((route) => route.model.capabilities.operations?.includes("conversation_respond"))
}

// groupByProvider splits selected rows into per-provider model id lists, keeping first-seen order.
// groupByProvider 将选中行按供应商拆成模型标识列表，保持首次出现顺序。
function groupByProvider(rows: readonly PublicProviderModelRow[]): Array<{ providerId: string; modelIds: string[] }> {
  const groups = new Map<string, string[]>()
  for (const row of rows) {
    const modelIds = groups.get(row.provider.id) ?? []
    modelIds.push(row.modelId)
    groups.set(row.provider.id, modelIds)
  }
  return Array.from(groups, ([providerId, modelIds]) => ({ providerId, modelIds }))
}

// ProviderModelCatalogProps selects which providers' models the catalog lists and how.
// ProviderModelCatalogProps 选择目录列出哪些供应商的模型以及展示方式。
export interface ProviderModelCatalogProps {
  // snapshot is the page-owned authoritative settings state.
  // snapshot 是页面持有的权威设置状态。
  snapshot: PublicAiSettingsSnapshot
  // providerIds lists the providers whose models appear, in display order.
  // providerIds 按显示顺序列出要展示其模型的供应商。
  providerIds: readonly string[]
  // showProvider adds the owning provider to each row (model-led view).
  // showProvider 在每行显示所属供应商（以模型为主的视图）。
  showProvider: boolean
  // accountScope narrows the list to one account and adds its access column.
  // accountScope 将列表收窄到一个账号并增加该账号的访问列。
  accountScope: PublicAiConnection | null
  // mutations is the page's single serialized write lane.
  // mutations 是页面唯一的串行写入通道。
  mutations: SnapshotMutations
  // onSnapshot applies snapshots saved by nested dialogs.
  // onSnapshot 应用嵌套弹窗保存后返回的快照。
  onSnapshot: (next: PublicAiSettingsSnapshot) => void
}

// ProviderModelCatalog lists provider models with per-model switches, filters, selection and bulk actions.
// ProviderModelCatalog 列出供应商模型，提供逐个开关、筛选、多选与批量操作。
export function ProviderModelCatalog(props: ProviderModelCatalogProps) {
  const { t } = useI18n()
  const { snapshot, mutations } = props
  const pendingMutationKey = mutations.pendingKey
  const [editingRoute, setEditingRoute] = React.useState<{ providerId: string; route: PublicModelProtocolRoute } | null>(null)
  // inspectingModel identifies a read-only access window by exact provider and model, never row position.
  // inspectingModel 使用准确供应商和模型标识接入详情窗口，不使用行位置。
  const [inspectingModel, setInspectingModel] = React.useState<{ providerId: string; modelId: string } | null>(null)
  // accessTrigger restores keyboard focus to the actual button that opened access details.
  // accessTrigger 将键盘焦点恢复到实际打开接入详情的按钮。
  const accessTrigger = React.useRef<HTMLButtonElement | null>(null)
  // routeConditionsTrigger restores focus within the parent access dialog after nested editing.
  // routeConditionsTrigger 在嵌套编辑结束后将焦点恢复到父级接入窗口内。
  const routeConditionsTrigger = React.useRef<HTMLButtonElement | null>(null)
  const [editingCapability, setEditingCapability] = React.useState<{ provider: PublicAiProviderDescriptor; model: PublicAiModel } | null>(null)
  const [searchQuery, setSearchQuery] = React.useState("")
  // usageFilter narrows visible rows using independent backend-owned admission dimensions.
  // usageFilter 使用相互独立的后端准入维度收窄可见模型。
  const [usageFilter, setUsageFilter] = React.useState("all")
  // capabilityFilter matches only declared output modes, including signed native extensions.
  // capabilityFilter 仅匹配已声明的输出模式，包括签名原生扩展。
  const [capabilityFilter, setCapabilityFilter] = React.useState("all")
  // currentPage bounds rendered model controls when a provider publishes a large catalog.
  // currentPage 在供应商发布大目录时限制一次渲染的模型控件数量。
  const [currentPage, setCurrentPage] = React.useState(1)
  // selected holds row keys chosen for a bulk action.
  // selected 保存为批量操作选中的行标识。
  const [selected, setSelected] = React.useState<ReadonlySet<string>>(() => new Set())

  // groups keeps credential-backed providers in the caller's order.
  // groups 按调用方顺序保留有凭据支持的供应商。
  const groups = React.useMemo(() => {
    const available = filterProviderModelInventoryGroupsWithAvailableAccounts(buildProviderModelInventoryGroups(snapshot))
    return props.providerIds.flatMap((providerId) => available.filter((group) => group.provider.id === providerId))
  }, [props.providerIds, snapshot])
  // accountSupported and accountEnabled are the scoped account's full and admitted model sets.
  // accountSupported 与 accountEnabled 是所选账号的完整模型集与已准入模型集。
  const accountSupported = React.useMemo(() => props.accountScope
    ? new Set((snapshot.modelSupportCatalogs[props.accountScope.id]?.models ?? []).map((model) => model.id))
    : null, [props.accountScope, snapshot])
  const accountEnabled = React.useMemo(() => props.accountScope
    ? new Set((snapshot.catalogs[props.accountScope.id]?.models ?? []).map((model) => model.id))
    : null, [props.accountScope, snapshot])
  // providerModelRows materializes effective model access state once per snapshot.
  // providerModelRows 为每次快照集中生成有效模型访问状态。
  const providerModelRows = React.useMemo<PublicProviderModelRow[]>(() => groups.flatMap((group) => (
    Array.from(group.models.values())
      .filter((inventory) => !accountSupported || accountSupported.has(inventory.modelId))
      .map((inventory) => {
        const access = snapshot.modelAccessRecords.find((record) => (
          record.providerId === group.provider.id && record.modelId === inventory.modelId
        ))
        return {
          provider: group.provider,
          key: `${group.provider.id}/${inventory.modelId}`,
          modelId: inventory.modelId,
          displayName: inventory.displayName,
          enabled: access?.enabled ?? true,
          routes: inventory.protocolRoutes,
          inventory,
        }
      })
  )), [accountSupported, groups, snapshot])
  // visibleModelRows applies page-local filters without changing catalog or persisted access state.
  // visibleModelRows 应用页面内筛选，不改变目录或持久化访问状态。
  const visibleModelRows = React.useMemo(
    () => filterProviderModels(providerModelRows, searchQuery, "all").filter((row) => {
      if (capabilityFilter !== "all" && ![
        ...row.inventory.catalogModels.flatMap((model) => providerModelModalities(model).output),
        ...providerExtensionOutputModalities(row.inventory.extensionCapabilities),
      ].includes(capabilityFilter as ReturnType<typeof providerModelModalities>["output"][number])) return false
      if (usageFilter === "all") return true
      if (usageFilter === "conversation") return row.enabled && rowConversationEligible(row)
      if (usageFilter === "account") return accountEnabled?.has(row.modelId) === true
      if (usageFilter === "internal") return internalStateForModel(snapshot, row.provider.id, row.modelId)?.enabled === true
        || row.inventory.extensionCapabilities.some((capability) => snapshot.providerToolEnabledCapabilities.includes(capability) && snapshot.providerExtensionTargetSets[capability]?.some((selection) => (
          snapshot.providerToolTargets.some((target) => target.connectionId === selection.connection_id && target.target.target_id === selection.target_id && target.target.provider_id === row.provider.id && target.target.upstream_target_id === row.modelId)
        )) === true)
      return snapshot.multimodalUnderstandingCandidateModels?.some((record) => record.providerId === row.provider.id && record.modelId === row.modelId) === true
    }),
    [accountEnabled, providerModelRows, searchQuery, usageFilter, capabilityFilter, snapshot],
  )
  // Changing any directory dimension begins at the first page and clears the selection.
  // 更换任意目录维度后从第一页开始并清空选择。
  const scopeKey = `${props.providerIds.join("\u0000")}|${props.accountScope?.id ?? ""}`
  React.useEffect(() => {
    setCurrentPage(1)
    setSelected(new Set())
  }, [scopeKey, searchQuery, usageFilter, capabilityFilter])
  React.useEffect(() => {
    if (!props.accountScope && usageFilter === "account") setUsageFilter("all")
  }, [props.accountScope, usageFilter])
  // pageSize limits simultaneous switches and tooltips for very large provider catalogs.
  // pageSize 限制超大供应商目录同时挂载的开关和提示组件数量。
  const pageSize = 50
  const pageCount = Math.max(1, Math.ceil(visibleModelRows.length / pageSize))
  const effectivePage = Math.min(currentPage, pageCount)
  // pageRows is the sole slice whose interactive controls are mounted.
  // pageRows 是唯一挂载交互控件的模型切片。
  const pageRows = visibleModelRows.slice((effectivePage - 1) * pageSize, effectivePage * pageSize)
  // selectedRows keeps only selections that are still visible after filtering.
  // selectedRows 只保留筛选后仍可见的选择。
  const selectedRows = visibleModelRows.filter((row) => selected.has(row.key))
  const pageSelection = pageRows.filter((row) => selected.has(row.key)).length
  // blockedModelRoutes keeps backend diagnostics visible without admitting blocked routes to controls.
  // blockedModelRoutes 保持后端诊断可见，同时不把受阻路由放入可操作控件。
  const blockedModelRoutes = React.useMemo(() => (snapshot.modelRouteRecords ?? []).filter((record) => (
    props.providerIds.includes(record.providerId) && !record.executable
    && (!props.accountScope || record.connectionId === props.accountScope.id)
  )), [props.accountScope, props.providerIds, snapshot])
  // inspectedRow reads current route metadata only while the listed providers still own the window.
  // inspectedRow 仅在所列供应商仍拥有窗口时读取最新路由元数据。
  const inspectedRow = inspectingModel
    ? providerModelRows.find((row) => row.provider.id === inspectingModel.providerId && row.modelId === inspectingModel.modelId)
    : undefined
  React.useEffect(() => {
    if (inspectingModel && !inspectedRow) {
      setInspectingModel(null)
      setEditingRoute(null)
    }
  }, [inspectedRow, inspectingModel])
  // unavailableAdmissions retain a cleanup path even when every route of an admitted model disappears.
  // unavailableAdmissions 在已准入模型的全部路由消失后仍保留清理入口。
  const unavailableAdmissions = snapshot.multimodalUnderstandingCandidateModels?.filter((record) => (
    props.providerIds.includes(record.providerId)
    && !groups.some((group) => group.provider.id === record.providerId && group.models.has(record.modelId))
  )) ?? []
  const providerLabel = (provider: PublicAiProviderDescriptor) => localizedProviderName(provider.id, provider.display_name, t)
  const run = mutations.run

  const toggleRow = (key: string, checked: boolean) => setSelected((current) => {
    const next = new Set(current)
    if (checked) next.add(key)
    else next.delete(key)
    return next
  })
  const selectRows = (rows: readonly PublicProviderModelRow[], checked: boolean) => setSelected((current) => {
    const next = new Set(current)
    for (const row of rows) {
      if (checked) next.add(row.key)
      else next.delete(row.key)
    }
    return next
  })

  return (
    <section className="settings-model-list-pane model-services-catalog-pane" aria-label={t("settings.models.liveModels")}>
      {unavailableAdmissions.length > 0 ? <section className="settings-model-unavailable-admissions" aria-label={t("settings.modelExperience.unavailableCandidates")}>
        <strong>{t("settings.modelExperience.unavailableCandidates")}</strong>
        {unavailableAdmissions.map((record) => <div className="settings-model-route-accounts" key={JSON.stringify([record.providerId, record.modelId])}>
          <span>{record.providerId} · {record.modelId}</span>
          <Button type="button" variant="outline" size="sm" disabled={pendingMutationKey !== null} onClick={() => void run(`multimodal-candidate/${record.providerId}/${record.modelId}`, () => savePublicAiMultimodalUnderstandingCandidate({ providerId: record.providerId, modelId: record.modelId, enabled: false }), { reconcile: true })}>{t("settings.modelExperience.disableCandidate")}</Button>
        </div>)}
      </section> : null}
      {blockedModelRoutes.length > 0 ? (
        <section aria-label={t("settings.models.blockedRouteDiagnostics")}>
          <div className="settings-split-column-title">
            {t("settings.models.blockedRouteDiagnostics")}
          </div>
          <div className="settings-model-row-list">
            {blockedModelRoutes.map((record) => {
              // blockedModel keeps the capability editor reachable while execution stays blocked.
              // blockedModel 让能力编辑入口保持可达，同时继续阻止模型执行。
              const blockedModel = snapshot.modelSupportCatalogs[record.connectionId]?.models.find(
                (model) => model.id === record.modelId,
              )
              const blockedProvider = snapshot.providers.find((provider) => provider.id === record.providerId)
              return (
                <article
                  className="settings-model-row"
                  key={JSON.stringify([record.connectionId, record.modelId])}
                >
                  <div className="settings-model-row-main">
                    <div className="settings-model-title-line">
                      <strong>{record.modelId}</strong>
                      <Badge variant="outline">{record.protocolId ?? "—"}</Badge>
                    </div>
                    {record.codexBehavior ? (
                      <span className="settings-model-route-binding">
                        {t("settings.models.codexBehavior")}: {codexBehaviorSummary(record.codexBehavior)}
                      </span>
                    ) : null}
                    {record.requiredRuntimePrimitive ? (
                      <span className="settings-model-route-binding">
                        {t("settings.models.requiredRuntimePrimitive")}: {record.requiredRuntimePrimitive}
                      </span>
                    ) : null}
                    <span className="settings-validation-error">
                      {t("settings.models.routeUnavailableReason")}: {record.unavailableReason ?? t("settings.models.routeUnresolved")}
                    </span>
                    {blockedProvider?.kind === "custom" && blockedModel ? <Button
                      aria-label={`${t("settings.modelCapabilities.title")} ${record.modelId}`}
                      disabled={pendingMutationKey !== null}
                      onClick={() => setEditingCapability({ provider: blockedProvider, model: blockedModel })}
                      size="sm"
                      type="button"
                      variant="outline"
                    >{t("settings.modelCapabilities.shortLabel")}</Button> : null}
                  </div>
                </article>
              )
            })}
          </div>
        </section>
      ) : null}
      <div className="settings-model-row-list">
        <div className="settings-model-sticky-controls">
          <div className="settings-model-toolbar">
            <div className="settings-model-search">
              <Search aria-hidden="true" size={15} />
              <Input
                aria-label={t("settings.models.searchLabel")}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder={t("settings.models.searchPlaceholder")}
                value={searchQuery}
              />
            </div>
            <Select value={usageFilter} onValueChange={setUsageFilter}>
              <SelectTrigger aria-label={t("settings.modelExperience.usageFilter")}><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("settings.modelExperience.allUsages")}</SelectItem>
                <SelectItem value="conversation">{t("settings.modelExperience.conversationEnabled")}</SelectItem>
                {props.accountScope ? <SelectItem value="account">{t("settings.modelServices.catalog.accountEnabled")}</SelectItem> : null}
                <SelectItem value="internal">{t("settings.modelExperience.internalEnabled")}</SelectItem>
                <SelectItem value="multimodal">{t("settings.modelExperience.multimodalEnabled")}</SelectItem>
              </SelectContent>
            </Select>
            <Select value={capabilityFilter} onValueChange={setCapabilityFilter}>
              <SelectTrigger aria-label={t("settings.modelExperience.capabilityFilter")}><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("settings.modelExperience.allCapabilities")}</SelectItem>
                {(["text", "image", "video", "audio", "file", "embedding", "rerank"] as const).map((modality) => <SelectItem key={modality} value={modality}>{modelModalityLabel(modality, t)}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          {selectedRows.length > 0 ? (
            <ModelCatalogBulkBar
              accountScope={props.accountScope}
              accountSupported={accountSupported}
              mutations={mutations}
              onClear={() => setSelected(new Set())}
              onSelectFiltered={() => selectRows(visibleModelRows, true)}
              selectedRows={selectedRows}
              snapshot={snapshot}
              visibleCount={visibleModelRows.length}
            />
          ) : (
            <div className="settings-model-filter-summary model-services-catalog-status">
              {t("settings.models.filteredSummary")
                .replace("{visible}", String(visibleModelRows.length))
                .replace("{total}", String(providerModelRows.length))}
            </div>
          )}
        </div>
        <div className="settings-model-catalog model-services-catalog" data-account-scope={props.accountScope ? "true" : "false"}>
          {visibleModelRows.length > 0 ? <div className="settings-model-catalog-heading">
            <span className="model-services-select-cell">
              <Checkbox
                aria-label={t("settings.modelServices.bulk.selectPage")}
                checked={pageSelection === 0 ? false : pageSelection === pageRows.length ? true : "indeterminate"}
                onCheckedChange={(checked) => selectRows(pageRows, checked === true)}
              />
            </span>
            <span aria-hidden="true">{t("settings.modelExperience.modelColumn")}</span>
            <span aria-hidden="true">{t("settings.models.conversationUse")}</span>
            {props.accountScope ? <span aria-hidden="true">{t("settings.modelServices.catalog.accountColumn")}</span> : null}
            <span aria-hidden="true">{t("settings.modelExperience.understandingUse")}</span>
            <span aria-hidden="true">{t("settings.models.internalUse")}</span>
            <span />
          </div> : null}
          {providerModelRows.length === 0 ? (
            <p className="settings-muted-line">{t("settings.models.emptyCatalog")}</p>
          ) : visibleModelRows.length === 0 ? (
            <p className="settings-muted-line">{t("settings.models.emptyFilter")}</p>
          ) : pageRows.map((row) => {
            const { modelId, enabled, displayName, inventory, provider } = row
            // representative supplies editable normalized capability facts when a catalog model exists.
            // representative 在目录模型存在时提供可编辑的归一化能力事实。
            const representative = inventory.catalogModels[0]
            const modelKey = row.key
            const conversationEligible = rowConversationEligible(row)
            const internalState = internalStateForModel(snapshot, provider.id, modelId)
            // multimodalCandidateEligible is backend-resolved across all executable account routes.
            // multimodalCandidateEligible 由后端跨全部可执行账号路由解析。
            const multimodalCandidateEligible = snapshot.mediaUnderstandingModelCandidates?.some(
              (candidate) => (
                candidate.selection.provider_id === provider.id
                && candidate.selection.model_id === modelId
              ),
            ) ?? false
            // multimodalCandidateEnabled is one shared provider-model switch, independent from conversation use.
            // multimodalCandidateEnabled 是与会话使用相互独立的共享供应商模型开关。
            const multimodalCandidateEnabled = snapshot.multimodalUnderstandingCandidateModels?.some(
              (record) => (
                record.providerId === provider.id
                && record.modelId === modelId
              ),
            ) ?? false
            const accountScope = props.accountScope
            const accountChecked = accountEnabled?.has(modelId) === true
            return (
              <article className="settings-model-catalog-row" data-selected={selected.has(modelKey) ? "true" : undefined} key={modelKey}>
                <span className="model-services-select-cell">
                  <Checkbox
                    aria-label={t("settings.modelServices.bulk.selectModel").replace("{model}", displayName)}
                    checked={selected.has(modelKey)}
                    onCheckedChange={(checked) => toggleRow(modelKey, checked === true)}
                  />
                </span>
                <div className="settings-model-row-main">
                  <div className="settings-model-title-line">
                    <strong title={displayName}>{displayName}</strong>
                  </div>
                  {/* Keep identity and capability metadata together on the second line.
                      将模型标识和能力摘要放在同一条第二行内。 */}
                  <div className="settings-model-catalog-meta">
                    {props.showProvider ? (
                      <span className="model-services-row-provider">
                        <ProviderIcon category={provider.category} className="size-3.5" providerId={provider.id} />
                        {providerLabel(provider)}
                      </span>
                    ) : null}
                    <span className="settings-model-id" title={modelId}>{modelId}</span>
                    <ProviderModelModalitySummary model={representative} additionalModels={inventory.catalogModels.slice(1)} extensionCapabilities={inventory.extensionCapabilities} />
                    <span>
                      {t("settings.models.availableCredentials")
                        .replace("{count}", String(inventory.availableAccountCount))}
                    </span>
                  </div>
                </div>
                <div className="settings-model-purpose-cell">
                  <span className="settings-model-purpose-label">{t("settings.models.conversationUse")}</span>
                  {conversationEligible ? (
                    <Switch
                      aria-label={`${t("settings.models.conversationUse")} ${displayName}`}
                      checked={enabled}
                      disabled={pendingMutationKey !== null}
                      onCheckedChange={(checked) => {
                        void run(`conversation/${modelKey}`, () => (
                          savePublicAiModelEnabled({
                            providerId: provider.id,
                            modelId,
                            enabled: checked,
                          })
                        ), { reconcile: true })
                      }}
                    />
                  ) : <span title={t("settings.modelExperience.noConversation")} aria-label={t("settings.modelExperience.noConversation")}>—</span>}
                </div>
                {accountScope ? (
                  <div className="settings-model-purpose-cell">
                    <span className="settings-model-purpose-label">{t("settings.modelServices.catalog.accountColumn")}</span>
                    <Switch
                      aria-label={`${t("settings.modelServices.catalog.accountColumn")} ${displayName}`}
                      checked={accountChecked}
                      disabled={pendingMutationKey !== null || (!accountChecked && conversationEligible && !enabled)}
                      onCheckedChange={(checked) => {
                        void run(`account/${accountScope.id}/${modelId}`, () => (
                          savePublicAiConnectionModelEnabled({ connectionId: accountScope.id, modelId, enabled: checked })
                        ), { reconcile: true })
                      }}
                    />
                  </div>
                ) : null}
                <div className="settings-model-purpose-cell">
                  <span className="settings-model-purpose-label">{t("settings.modelExperience.understandingUse")}</span>
                  {/* Tooltip disclosure state belongs to its wrapper, preserving the switch's checked state.
                      提示层的展开状态由外层元素承担，保留开关自身的选中状态。 */}
                  {multimodalCandidateEligible || multimodalCandidateEnabled ? (
                    <Tooltip><TooltipTrigger asChild><span className="inline-flex">
                      <Switch
                        aria-label={`${t("settings.models.multimodalUnderstandingCandidate")} ${displayName}`}
                        aria-description={t("settings.modelExperience.candidateExplanation")}
                        checked={multimodalCandidateEnabled}
                        disabled={multimodalCandidateToggleDisabled(
                          pendingMutationKey !== null,
                          multimodalCandidateEligible,
                          multimodalCandidateEnabled,
                        )}
                        onCheckedChange={(checked) => {
                          void run(`multimodal-candidate/${modelKey}`, () => (
                            savePublicAiMultimodalUnderstandingCandidate({
                              providerId: provider.id,
                              modelId,
                              enabled: checked,
                            })
                          ), { reconcile: true })
                        }}
                      />
                    </span></TooltipTrigger><TooltipContent>{t("settings.modelExperience.candidateExplanation")}</TooltipContent></Tooltip>
                  ) : <span>—</span>}
                </div>
                <div className="settings-model-purpose-cell settings-model-extension-cell">
                  <span className="settings-model-purpose-label">{t("settings.models.internalUse")}</span>
                  {internalState ? (
                    <Tooltip><TooltipTrigger asChild><span className="inline-flex">
                      <Switch
                        aria-label={`${t("settings.models.internalUse")} ${displayName}`}
                        aria-description={internalState.operations.map((operation) => internalOperationLabel(operation, t)).join(t("global.listDelimiter"))}
                        checked={internalState.enabled}
                        disabled={pendingMutationKey !== null}
                        onCheckedChange={(checked) => {
                          void run(`internal/${modelKey}`, () => (
                            savePublicAiInternalModelEnabled({
                              providerId: provider.id,
                              modelId,
                              enabled: checked,
                            })
                          ), { reconcile: true })
                        }}
                      />
                    </span></TooltipTrigger><TooltipContent>{t("settings.modelExperience.internalPurposes").replace("{purposes}", internalState.operations.map((operation) => internalOperationLabel(operation, t)).join(t("global.listDelimiter")))}</TooltipContent></Tooltip>
                  ) : null}
                  {inventory.extensionCapabilities.filter(isModelExtensionCapability).map((capability) => {
                    // Matching exact targets preserves the extension's persisted account choice.
                    // 匹配精确目标以保留扩展已持久化的账号选择。
                    const targets = extensionModelTargets(snapshot, provider.id, modelId, capability)
                    // selections contains the complete saved model set for this extension capability.
                    // selections 包含此扩展能力已保存的完整模型集合。
                    const selections = snapshot.providerExtensionTargetSets[capability] ?? []
                    // selectedTarget identifies this model's exact account selection, if present.
                    // selectedTarget 标识此模型已选中的精确账号，如有。
                    const selectedTarget = selections.find((selection) => targets.some((target) => (
                      target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
                    )))
                    // available is a signed target with the reviewed extension profile required by the backend.
                    // available 是包含后端所需审核扩展资料的签名目标。
                    const available = targets.find((target) => target.target.contract.extension_profile !== undefined)
                    const label = extensionCapabilityLabel(capability, t)
                    return <Tooltip key={capability}><TooltipTrigger asChild><span className="settings-model-extension-toggle">
                      <Switch
                        aria-label={`${label} ${displayName}`}
                        checked={selectedTarget !== undefined}
                        disabled={pendingMutationKey !== null || (!available && !selectedTarget)}
                        onCheckedChange={(checked) => {
                          // retained preserves every other model in the capability's complete target set.
                          // retained 保留此能力完整目标集合中的所有其他模型。
                          const retained = selections.filter((selection) => !targets.some((target) => (
                            target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
                          )))
                          if (checked && available) retained.push({ connection_id: available.connectionId, target_id: available.target.target_id })
                          void run(`extension/${capability}/${modelKey}`, async () => {
                            // next reflects the persisted selection before global capability admission.
                            // next 反映全局能力准入前已经持久化的模型选择。
                            const next = await savePublicAiProviderExtensionTargetSet({ capability, selections: retained })
                            if (checked && !next.providerToolEnabledCapabilities.includes(capability)) {
                              return savePublicAiProviderToolCapabilityEnabled({ capability, enabled: true })
                            }
                            return next
                          }, { reconcile: true })
                        }}
                      />
                      <small>{label}</small>
                    </span></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
                  })}
                  {!internalState && !inventory.extensionCapabilities.some(isModelExtensionCapability) ? <span>—</span> : null}
                </div>
                <div className="settings-model-catalog-actions">
                  {provider.kind === "custom" && representative ? <Button
                    aria-label={`${t("settings.modelCapabilities.title")} ${displayName}`}
                    disabled={pendingMutationKey !== null}
                    onClick={() => setEditingCapability({ provider, model: representative })}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {t("settings.modelCapabilities.shortLabel")}
                  </Button> : null}
                  <Button
                    aria-label={`${t("settings.modelExperience.accessButton")} ${displayName}`}
                    onClick={(event) => {
                      accessTrigger.current = event.currentTarget
                      setInspectingModel({ providerId: provider.id, modelId })
                    }}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >{t("settings.modelExperience.accessButton")}</Button>
                </div>
              </article>
            )
          })}
        </div>
        {pageCount > 1 ? <div className="settings-pagination settings-model-pagination">
          <Button disabled={effectivePage <= 1} onClick={() => setCurrentPage(effectivePage - 1)} size="sm" type="button" variant="outline">{t("settings.modelExperience.previousPage")}</Button>
          <span>{t("settings.modelExperience.pageStatus").replace("{page}", String(effectivePage)).replace("{count}", String(pageCount))}</span>
          <Button disabled={effectivePage >= pageCount} onClick={() => setCurrentPage(effectivePage + 1)} size="sm" type="button" variant="outline">{t("settings.modelExperience.nextPage")}</Button>
        </div> : null}
      </div>
      <Dialog open={inspectedRow !== undefined} onOpenChange={(open) => {
        if (!open) {
          setInspectingModel(null)
          setEditingRoute(null)
        }
      }}>
        <DialogContent size="lg" scrollMode="body-scroll" className="settings-model-access-dialog" onCloseAutoFocus={(event) => {
          if (accessTrigger.current?.isConnected) {
            event.preventDefault()
            accessTrigger.current.focus()
          }
        }}>
          <DialogHeader>
            <DialogTitle>{inspectedRow?.displayName}</DialogTitle>
            <DialogDescription>{inspectedRow?.routes.length ? t("settings.modelExperience.routes").replace("{count}", String(inspectedRow.routes.length)) : t("settings.modelExperience.nativeInventory")}</DialogDescription>
          </DialogHeader>
          <div className="settings-model-access-routes">
            {inspectedRow && inspectedRow.routes.length === 0 ? <section className="settings-model-access-route">
              <ProviderModelModalitySummary model={inspectedRow.inventory.catalogModels[0]} additionalModels={inspectedRow.inventory.catalogModels.slice(1)} extensionCapabilities={inspectedRow.inventory.extensionCapabilities} />
              <p>{t("settings.models.availableCredentials").replace("{count}", String(inspectedRow.inventory.availableAccountCount))}</p>
            </section> : null}
            {inspectedRow?.routes.map((route) => <section className="settings-model-access-route" key={modelRouteValue(route)}>
              <div className="settings-model-access-route-heading">
                <div><strong>{route.serviceEntryName}</strong><span>{protocolDisplayName(route.protocolId)}</span></div>
                <ProviderModelModalitySummary model={route.model} />
                <Button
                  aria-label={t("settings.models.additionalConditions").replace("{protocol}", protocolDisplayName(route.protocolId))}
                  disabled={pendingMutationKey !== null}
                  onClick={(event) => {
                    routeConditionsTrigger.current = event.currentTarget
                    setEditingRoute({ providerId: inspectedRow.provider.id, route })
                  }}
                  size="sm"
                  type="button"
                  variant="outline"
                ><SlidersHorizontal aria-hidden="true" size={14} />{t("settings.modelExperience.routeSettings")}</Button>
              </div>
              <dl className="settings-model-access-metadata">
                <div><dt>{t("settings.modelExperience.bindingSource")}</dt><dd>{route.bindingSource} · {route.bindingRuleId ?? t("settings.models.routeNoRule")} · r{route.manifestRevision}</dd></div>
                <div><dt>{t("settings.models.routeAvailableProtocols")}</dt><dd>{route.availableProtocols.join(", ") || "—"}</dd></div>
              </dl>
              <div className="settings-model-access-accounts">
                <span>{t("settings.modelExperience.routeAccounts")}</span>
                <ul className="settings-model-route-accounts">
                  {route.accounts.map((account) => <li key={account.connectionId}>
                    <span>{account.displayName}</span>
                    <small>{t(account.enabled ? "settings.modelExperience.available" : "settings.modelExperience.accountNotAdmitted")}</small>
                  </li>)}
                </ul>
              </div>
            </section>)}
          </div>
          <PublicAiModelOverridesDialog
            model={editingRoute?.route.model ?? null}
            onCloseAutoFocus={(event) => {
              if (routeConditionsTrigger.current?.isConnected) {
                event.preventDefault()
                routeConditionsTrigger.current.focus()
              }
            }}
            onOpenChange={(open) => {
              if (!open) {
                setEditingRoute(null)
              }
            }}
            onSaved={(next) => {
              props.onSnapshot(next)
              setEditingRoute(null)
            }}
            open={editingRoute !== null}
            overrides={snapshot.modelOverrideRecords.find((record) => (
              record.providerId === editingRoute?.providerId
              && record.modelId === editingRoute?.route.model.id
              && record.protocolId === editingRoute?.route.protocolId
            ))?.overrides}
            protocolId={editingRoute?.route.protocolId ?? null}
            providerDisplayName={inspectedRow ? providerLabel(inspectedRow.provider) : null}
            providerId={editingRoute?.providerId ?? null}
          />
        </DialogContent>
      </Dialog>
      <PublicAiModelCapabilityDialog
        model={editingCapability?.model ?? null}
        onOpenChange={(open) => {
          if (!open) {
            setEditingCapability(null)
          }
        }}
        onSaved={(next) => {
          props.onSnapshot(next)
          setEditingCapability(null)
        }}
        open={editingCapability !== null}
        overrides={snapshot.modelCapabilityOverrideRecords?.find((record) => (
          record.providerId === editingCapability?.provider.id
          && record.modelId === editingCapability?.model.id
        ))?.overrides}
        providerDisplayName={editingCapability ? providerLabel(editingCapability.provider) : null}
        providerId={editingCapability?.provider.id ?? null}
      />
    </section>
  )
}

// ModelCatalogBulkBarProps carries the selection and the scope a bulk action writes to.
// ModelCatalogBulkBarProps 承载批量操作写入的选择与范围。
interface ModelCatalogBulkBarProps {
  snapshot: PublicAiSettingsSnapshot
  selectedRows: PublicProviderModelRow[]
  visibleCount: number
  accountScope: PublicAiConnection | null
  accountSupported: ReadonlySet<string> | null
  mutations: SnapshotMutations
  onSelectFiltered: () => void
  onClear: () => void
}

// ModelCatalogBulkBar applies one change to every eligible selected model and reports the skipped ones.
// ModelCatalogBulkBar 将一项变更应用到每个符合条件的选中模型，并报告跳过的数量。
function ModelCatalogBulkBar(props: ModelCatalogBulkBarProps) {
  const { t } = useI18n()
  const { snapshot, selectedRows, mutations } = props
  // notice reports models a bulk action skipped because they lack the capability.
  // notice 报告批量操作因模型不支持而跳过的数量。
  const [notice, setNotice] = React.useState<string | null>(null)
  const busy = mutations.pendingKey !== null
  const runBulk = (
    key: string,
    eligible: (row: PublicProviderModelRow) => boolean,
    write: (rows: PublicProviderModelRow[]) => Promise<PublicAiSettingsSnapshot | void>,
  ) => {
    const rows = selectedRows.filter(eligible)
    const skipped = selectedRows.length - rows.length
    setNotice(skipped > 0 ? t("settings.modelServices.bulk.skipped").replace("{count}", String(skipped)) : null)
    if (rows.length === 0) return
    void mutations.run(key, () => write(rows), { reconcile: true })
  }
  // Session availability and internal use are saved per provider, one call each.
  // 会话可用性与内部使用按供应商保存，每个供应商一次调用。
  const bulkConversation = (enabled: boolean) => runBulk(`bulk/conversation/${enabled}`, rowConversationEligible, async (rows) => {
    let next: PublicAiSettingsSnapshot | undefined
    for (const group of groupByProvider(rows)) next = await savePublicAiModelSetEnabled({ ...group, enabled })
    return next
  })
  const bulkInternal = (enabled: boolean) => runBulk(
    `bulk/internal/${enabled}`,
    (row) => internalStateForModel(snapshot, row.provider.id, row.modelId) !== null,
    async (rows) => {
      let next: PublicAiSettingsSnapshot | undefined
      for (const group of groupByProvider(rows)) next = await savePublicAiInternalModelSetEnabled({ ...group, enabled })
      return next
    },
  )
  // bulkAccount saves the scoped account's access for every selected model it supports.
  // bulkAccount 为所选账号支持的每个选中模型保存访问状态。
  const bulkAccount = (enabled: boolean) => {
    const account = props.accountScope
    if (!account) return
    runBulk(
      `bulk/account/${enabled}`,
      (row) => props.accountSupported?.has(row.modelId) === true && (!enabled || row.enabled || !rowConversationEligible(row)),
      (rows) => savePublicAiConnectionModelSetEnabled({ connectionId: account.id, modelIds: rows.map((row) => row.modelId), enabled }),
    )
  }
  const group = (label: string, apply: (enabled: boolean) => void) => (
    <span className="model-services-bulk-group">
      <span>{label}</span>
      <Button disabled={busy} onClick={() => apply(true)} size="sm" type="button" variant="outline">{t("settings.modelServices.bulk.enable")}</Button>
      <Button disabled={busy} onClick={() => apply(false)} size="sm" type="button" variant="outline">{t("settings.modelServices.bulk.disable")}</Button>
    </span>
  )
  return (
    <>
      <div className="model-services-bulk-bar" role="toolbar" aria-label={t("settings.modelServices.bulk.title")}>
        <strong>{t("settings.modelServices.bulk.selected").replace("{count}", String(selectedRows.length))}</strong>
        {selectedRows.length < props.visibleCount ? (
          <Button onClick={props.onSelectFiltered} size="sm" type="button" variant="link">
            {t("settings.modelServices.bulk.selectFiltered").replace("{count}", String(props.visibleCount))}
          </Button>
        ) : null}
        {group(t("settings.models.conversationUse"), bulkConversation)}
        {group(t("settings.models.internalUse"), bulkInternal)}
        {props.accountScope ? group(t("settings.modelServices.catalog.accountColumn"), bulkAccount) : null}
        <Button onClick={props.onClear} size="sm" type="button" variant="ghost">{t("settings.modelServices.bulk.clear")}</Button>
      </div>
      {notice ? <p className="settings-model-filter-summary" role="status">{notice}</p> : null}
    </>
  )
}

// codexBehaviorSummary formats the exact backend behavior without deriving new capability claims.
// codexBehaviorSummary 格式化后端精确行为，不派生新的能力声明。
function codexBehaviorSummary(
  behavior: PublicAiCodexModelBehavior,
): string {
  return [
    behavior.wire_dialect,
    behavior.tool_mode,
    behavior.apply_patch_tool_type,
    behavior.shell_type,
    behavior.multi_agent_version,
  ].filter((value) => value !== undefined).join(" · ")
}

// modelRouteValue returns the stable tab value for one exact service-entry protocol route.
// modelRouteValue 返回一个精确服务入口协议路由的稳定页签值。
function modelRouteValue(route: PublicModelProtocolRoute): string {
  return JSON.stringify([
    route.serviceEntryId,
    route.protocolId,
    route.bindingSource,
    route.bindingRuleId ?? null,
    route.manifestRevision,
    route.availableProtocols,
  ])
}

// InternalModelState describes one model-level internal-use switch and its available operations.
// InternalModelState 描述一个模型级内部使用开关及其可用操作。
export interface InternalModelState {
  // Effective model-level internal-use state.
  // 生效的模型级内部使用状态。
  enabled: boolean
  // Deduplicated internal operations exposed by configured routes.
  // 已配置路由公开的去重内部操作。
  operations: PublicAiOperation[]
}

// internalStateForModel returns one shared internal-use state and its operation capabilities.
// internalStateForModel 返回一个共享内部使用状态及其操作能力。
export function internalStateForModel(
  snapshot: Pick<PublicAiSettingsSnapshot, "internalInferenceCandidates">,
  providerId: string,
  modelId: string,
): InternalModelState | null {
  const operations = new Set<PublicAiOperation>()
  let enabled = false
  for (const candidate of snapshot.internalInferenceCandidates) {
    if (
      candidate.route.provider_id === providerId
      && candidate.route.model_id === modelId
    ) {
      operations.add(candidate.route.operation)
      enabled ||= candidate.enabled
    }
  }
  return operations.size > 0
    ? { enabled, operations: Array.from(operations) }
    : null
}

// multimodalCandidateToggleDisabled blocks concurrent writes and invalid new admissions while preserving stale-state cleanup.
// multimodalCandidateToggleDisabled 阻止并发写入和无效的新准入，同时保留清理失效旧状态的能力。
// `pending` reports an active settings mutation, `eligible` reports a currently executable route, and `enabled` reports persisted admission; the return value controls the switch disabled state.
// `pending` 表示设置写入进行中，`eligible` 表示当前存在可执行路由，`enabled` 表示已持久化准入；返回值控制开关禁用状态。
export function multimodalCandidateToggleDisabled(
  pending: boolean,
  eligible: boolean,
  enabled: boolean,
): boolean {
  return pending || (!eligible && !enabled)
}

// internalOperationLabel returns the localized compact operation name.
// internalOperationLabel 返回本地化的紧凑操作名称。
function internalOperationLabel(
  operation: PublicAiOperation,
  t: ReturnType<typeof useI18n>["t"],
): string {
  switch (operation) {
    case "conversation_respond":
      return t("settings.inference.llm")
    case "embedding_create":
      return t("settings.inference.embedding")
    case "rerank_documents":
      return t("settings.inference.rerank")
    case "decision_evaluate":
      return t("settings.inference.decision")
  }
}

// protocolDisplayName maps exact protocol profile identifiers to user-visible labels.
// protocolDisplayName 把精确协议配置档标识映射为用户可见标签。
function protocolDisplayName(protocolId: string): string {
  const labels: Record<string, string> = {
    "anthropic-messages.v1": "Anthropic",
    "anthropic-messages-video.v1": "Anthropic + Video",
    "claude-code-messages.v1": "Claude Code",
    "openai-chat-completions.v1": "OpenAI Chat",
    "openai-codex-responses.v1": "Codex Responses",
    "openai-responses.v1": "OpenAI Responses",
  }
  return labels[protocolId] ?? protocolId
}
