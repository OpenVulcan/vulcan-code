import { WorkspaceDirectoryLayout, WorkspaceDirectoryMenu } from "@/components/workspace/workspace-directory-layout";
import * as React from "react"
import {
  Search,
  SlidersHorizontal,
} from "lucide-react"

import {
  filterProviderModels,
  type FilterableProviderModel,
} from "@/components/features/settings/provider-model-filter"
import { modelModalityLabel, ProviderModelModalitySummary } from "@/components/features/settings/provider-model-modality-summary"
import { providerExtensionOutputModalities, providerModelModalities } from "@/components/features/settings/provider-model-modalities"
import { providerPriority } from "@/components/features/settings/provider-ordering"
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
import { Combobox } from "@/components/ui/combobox"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiInternalModelEnabled,
  savePublicAiMultimodalUnderstandingCandidate,
  savePublicAiModelEnabled,
  savePublicAiProviderExtensionTargetSet,
  savePublicAiProviderToolCapabilityEnabled,
} from "@/service/settings-actions-service"
import type {
  PublicAiCodexModelBehavior,
  PublicAiModel,
  PublicAiOperation,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTargetSnapshot,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicProviderModelRow binds one rendered provider model to its routes and effective enabled state.
// PublicProviderModelRow 将一个渲染中的供应商模型绑定到其路由与有效启用状态。
interface PublicProviderModelRow extends FilterableProviderModel {
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

// ModelListSettingsArea renders the process-local public AI model catalog.
// ModelListSettingsArea 渲染进程内公共 AI 模型目录。
export function ModelListSettingsArea() {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // loadAttempt retries a failed catalog load only on explicit user action.
  // loadAttempt 仅在用户显式操作时重试失败的目录加载。
  const [loadAttempt, setLoadAttempt] = React.useState(0)
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [editingRoute, setEditingRoute] = React.useState<PublicModelProtocolRoute | null>(null)
  // inspectingModel identifies a read-only access window by exact provider and model, never row position.
  // inspectingModel 使用准确供应商和模型标识接入详情窗口，不使用行位置。
  const [inspectingModel, setInspectingModel] = React.useState<{ providerId: string; modelId: string } | null>(null)
  // accessTrigger restores keyboard focus to the actual button that opened access details.
  // accessTrigger 将键盘焦点恢复到实际打开接入详情的按钮。
  const accessTrigger = React.useRef<HTMLButtonElement | null>(null)
  // routeConditionsTrigger restores focus within the parent access dialog after nested editing.
  // routeConditionsTrigger 在嵌套编辑结束后将焦点恢复到父级接入窗口内。
  const routeConditionsTrigger = React.useRef<HTMLButtonElement | null>(null)
  const [editingCapabilityModel, setEditingCapabilityModel] = React.useState<PublicAiModel | null>(null)
  const [pendingMutationKey, setPendingMutationKey] = React.useState<string | null>(null)
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
  const mutationInFlightRef = React.useRef(false)

  // runSnapshotMutation serializes full-snapshot writes so late responses cannot regress the UI.
  // runSnapshotMutation 串行化完整快照写入，避免迟到响应使界面状态倒退。
  const runSnapshotMutation = React.useCallback((
    key: string,
    mutation: () => Promise<PublicAiSettingsSnapshot>,
  ) => {
    if (mutationInFlightRef.current) {
      return
    }
    mutationInFlightRef.current = true
    setPendingMutationKey(key)
    void mutation()
      .then((next) => {
        setSnapshot(next)
        setError(null)
      })
      .catch(async (reason) => {
        setError(errorMessage(reason))
        // A two-step extension write may have saved its selection before global enablement fails.
        // 两步扩展写入可能在全局启用失败前已保存选择，因此重新读取权威状态。
        try {
          setSnapshot(await loadPublicAiSettings())
        } catch {
          // The original mutation error remains visible when reconciliation cannot load.
          // 对账读取失败时保留原写入错误供用户查看。
        }
      })
      .finally(() => {
        mutationInFlightRef.current = false
        setPendingMutationKey(null)
      })
  }, [])

  React.useEffect(() => {
    let active = true
    setError(null)
    void loadPublicAiSettings()
      .then((next) => {
        if (active) {
          setSnapshot(next)
          setError(null)
        }
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason))
        }
      })
    return () => {
      active = false
    }
  }, [loadAttempt])

  // groupedProviders keeps credential-backed model providers ordered by persisted priority.
  // groupedProviders 按持久化优先级排列由凭据支持的模型供应商。
  const groupedProviders = React.useMemo(() => {
    if (!snapshot) {
      return []
    }
    // indexedGroups preserves current registry order when priorities are equal.
    // indexedGroups 在优先级相同时保留当前注册表顺序。
    const indexedGroups = filterProviderModelInventoryGroupsWithAvailableAccounts(
      buildProviderModelInventoryGroups(snapshot),
    )
      .map((group, index) => ({ group, index }))
    return indexedGroups
      .sort((left, right) => (
        providerPriority(snapshot.providerPriorities, right.group.provider.id)
        - providerPriority(snapshot.providerPriorities, left.group.provider.id)
        || left.index - right.index
      ))
      .map(({ group }) => group)
  }, [snapshot])
  React.useEffect(() => {
    if (groupedProviders.length === 0) {
      setSelectedProviderId(null)
      return
    }
    if (!selectedProviderId || !groupedProviders.some((group) => group.provider.id === selectedProviderId)) {
      setSelectedProviderId(groupedProviders[0]?.provider.id ?? null)
    }
  }, [groupedProviders, selectedProviderId])

  const selectedGroup = groupedProviders.find((group) => group.provider.id === selectedProviderId)
    ?? groupedProviders[0]
    ?? null
  // providerModelRows materializes the selected provider's effective model access state once per snapshot.
  // providerModelRows 为每次快照集中生成当前供应商的有效模型访问状态。
  const providerModelRows = React.useMemo<PublicProviderModelRow[]>(() => {
    if (!snapshot || !selectedGroup) {
      return []
    }
    return Array.from(selectedGroup.models.values()).map((inventory) => {
      const { modelId, protocolRoutes: routes } = inventory
      const access = snapshot.modelAccessRecords.find((record) => (
        record.providerId === selectedGroup.provider.id
        && record.modelId === modelId
      ))
      return {
        modelId,
        displayName: inventory.displayName,
        enabled: access?.enabled ?? true,
        routes,
        inventory,
      }
    })
  }, [selectedGroup, snapshot])
  // visibleModelRows applies page-local filters without changing catalog or persisted access state.
  // visibleModelRows 应用页面内筛选，不改变目录或持久化访问状态。
  const visibleModelRows = React.useMemo(
    () => filterProviderModels(providerModelRows, searchQuery, "all").filter((row) => {
      if (capabilityFilter !== "all" && ![
        ...row.inventory.catalogModels.flatMap((model) => providerModelModalities(model).output),
        ...providerExtensionOutputModalities(row.inventory.extensionCapabilities),
      ].includes(capabilityFilter as ReturnType<typeof providerModelModalities>["output"][number])) return false
      if (usageFilter === "all") return true
      if (usageFilter === "conversation") return row.enabled && row.routes.some((route) => route.model.capabilities.operations?.includes("conversation_respond"))
      if (!snapshot || !selectedGroup) return false
      if (usageFilter === "internal") return internalStateForModel(snapshot, selectedGroup.provider.id, row.modelId)?.enabled === true
        || row.inventory.extensionCapabilities.some((capability) => snapshot.providerToolEnabledCapabilities.includes(capability) && snapshot.providerExtensionTargetSets[capability]?.some((selection) => (
          snapshot.providerToolTargets.some((target) => target.connectionId === selection.connection_id && target.target.target_id === selection.target_id && target.target.provider_id === selectedGroup.provider.id && target.target.upstream_target_id === row.modelId)
        )) === true)
      return snapshot.multimodalUnderstandingCandidateModels?.some((record) => record.providerId === selectedGroup.provider.id && record.modelId === row.modelId) === true
    }),
    [providerModelRows, searchQuery, usageFilter, capabilityFilter, snapshot, selectedGroup],
  )
  // Changing any directory dimension begins at the first page of the new result set.
  // 更换任意目录维度后从新结果集第一页开始。
  React.useEffect(() => setCurrentPage(1), [selectedProviderId, searchQuery, usageFilter, capabilityFilter])
  // pageSize limits simultaneous switches and tooltips for very large provider catalogs.
  // pageSize 限制超大供应商目录同时挂载的开关和提示组件数量。
  const pageSize = 50
  // pageCount retains at least one logical page for an empty filtered set.
  // pageCount 为筛选结果为空的情况保留至少一个逻辑页。
  const pageCount = Math.max(1, Math.ceil(visibleModelRows.length / pageSize))
  // effectivePage clamps a stale page index after a catalog refresh.
  // effectivePage 在目录刷新后约束过期页码。
  const effectivePage = Math.min(currentPage, pageCount)
  // pageRows is the sole slice whose interactive controls are mounted.
  // pageRows 是唯一挂载交互控件的模型切片。
  const pageRows = visibleModelRows.slice((effectivePage - 1) * pageSize, effectivePage * pageSize)
  // blockedModelRoutes keeps backend diagnostics visible without admitting blocked routes to controls.
  // blockedModelRoutes 保持后端诊断可见，同时不把受阻路由放入可操作控件。
  const blockedModelRoutes = React.useMemo(() => {
    if (!snapshot || !selectedGroup) {
      return []
    }
    return (snapshot.modelRouteRecords ?? []).filter((record) => (
      record.providerId === selectedGroup.provider.id && !record.executable
    ))
  }, [selectedGroup, snapshot])
  // inspectedRow reads current route metadata only while the selected provider still owns the window.
  // inspectedRow 仅在当前供应商仍拥有窗口时读取最新路由元数据。
  const inspectedRow = inspectingModel?.providerId === selectedGroup?.provider.id
    ? providerModelRows.find((row) => row.modelId === inspectingModel?.modelId)
    : undefined
  React.useEffect(() => {
    if (inspectingModel && !inspectedRow) {
      setInspectingModel(null)
      setEditingRoute(null)
    }
  }, [inspectedRow, inspectingModel])
  // selectedProviderLabel supplies the shared collapsed-menu pill label.
  // selectedProviderLabel 为共享收起菜单胶囊提供标签。
  const selectedProviderLabel = selectedGroup
    ? localizedProviderName(selectedGroup.provider.id, selectedGroup.provider.display_name, t)
    : t("settings.models.noProviderSelected")
  // unavailableAdmissions retain a cleanup path even when every route of an admitted model disappears.
  // unavailableAdmissions 在已准入模型的全部路由消失后仍保留清理入口。
  const unavailableAdmissions = snapshot?.multimodalUnderstandingCandidateModels?.filter((record) => (
    !groupedProviders.some((group) => group.provider.id === record.providerId && group.models.has(record.modelId))
  )) ?? []
  return (
    <WorkspaceDirectoryLayout contentClassName="model-list-layout model-list-surface" navigation={<WorkspaceDirectoryMenu ariaLabel={t("settings.models.provider")} heading={t("settings.models.provider")}>
          {groupedProviders.map((group) => (
            <button
              className={`workspace-tool-panel-menu-row ${selectedGroup?.provider.id === group.provider.id ? "active" : ""}`}
              key={group.provider.id}
              onClick={() => setSelectedProviderId(group.provider.id)}
              aria-current={selectedGroup?.provider.id === group.provider.id ? "page" : undefined}
              type="button"
            >
              <ProviderIcon
                category={group.provider.category}
                className="size-5"
                providerId={group.provider.id}
              />
              <span className="settings-directory-label" title={localizedProviderName(group.provider.id, group.provider.display_name, t)}>{localizedProviderName(group.provider.id, group.provider.display_name, t)}</span>
              <small>{group.models.size}</small>
            </button>
          ))}
        </WorkspaceDirectoryMenu>}>
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        {!snapshot && error ? <Button onClick={() => setLoadAttempt((value) => value + 1)} type="button" variant="outline">{t("settings.modelExperience.retryLoad")}</Button> : !snapshot ? (
          <p className="settings-muted-line">{t("settings.models.loadingCatalog")}</p>
        ) : (
          <section className="settings-model-list-pane" aria-label={t("settings.models.liveModels")}>
            <div className="settings-model-pane-header">
              <div>
                <div className="settings-split-column-title">{t("settings.models.liveModels")}</div>
                <small>
                  {t("settings.models.catalogSummary")
                    .replace("{provider}", selectedProviderLabel)
                    .replace("{count}", String(providerModelRows.length))}
                </small>
              </div>
              <span className="settings-muted-line">{t("settings.models.memoryOnly")}</span>
            </div>
            {unavailableAdmissions.length > 0 ? <section className="settings-model-unavailable-admissions" aria-label={t("settings.modelExperience.unavailableCandidates")}>
              <strong>{t("settings.modelExperience.unavailableCandidates")}</strong>
              {unavailableAdmissions.map((record) => <div className="settings-model-route-accounts" key={JSON.stringify([record.providerId, record.modelId])}>
                <span>{record.providerId} · {record.modelId}</span>
                <Button type="button" variant="outline" size="sm" disabled={pendingMutationKey !== null} onClick={() => runSnapshotMutation(`multimodal-candidate/${record.providerId}/${record.modelId}`, () => savePublicAiMultimodalUnderstandingCandidate({ providerId: record.providerId, modelId: record.modelId, enabled: false }))}>{t("settings.modelExperience.disableCandidate")}</Button>
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
                          {blockedModel ? <Button
                            aria-label={`${t("settings.modelCapabilities.title")} ${record.modelId}`}
                            disabled={pendingMutationKey !== null}
                            onClick={() => setEditingCapabilityModel(blockedModel)}
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
                <div className="settings-model-filter-summary">
                  {t("settings.models.filteredSummary")
                    .replace("{visible}", String(visibleModelRows.length))
                    .replace("{total}", String(providerModelRows.length))}
                </div>
              </div>
              <div className="settings-model-catalog">
              {visibleModelRows.length > 0 ? <div className="settings-model-catalog-heading" aria-hidden="true">
                <span>{t("settings.modelExperience.modelColumn")}</span>
                <span>{t("settings.models.conversationUse")}</span>
                <span>{t("settings.modelExperience.understandingUse")}</span>
                <span>{t("settings.models.internalUse")}</span>
                <span />
              </div> : null}
              {!selectedGroup || providerModelRows.length === 0 ? (
                <p className="settings-muted-line">{t("settings.models.emptyCatalog")}</p>
              ) : visibleModelRows.length === 0 ? (
                <p className="settings-muted-line">{t("settings.models.emptyFilter")}</p>
              ) : pageRows.map(({ modelId, routes, enabled, displayName, inventory }) => {
                // representative supplies editable normalized capability facts when a catalog model exists.
                // representative 在目录模型存在时提供可编辑的归一化能力事实。
                const representative = inventory.catalogModels[0]
                // availableAccountCount has already deduplicated catalog and extension accounts.
                // availableAccountCount 已经对目录与扩展账号去重。
                const availableAccountCount = inventory.availableAccountCount
                const modelKey = `${selectedGroup.provider.id}/${modelId}`
                // conversationEligible follows declared operations, independently from its access preference.
                // conversationEligible 遵循声明操作，与访问偏好相互独立。
                const conversationEligible = routes.some((route) => route.model.capabilities.operations?.includes("conversation_respond"))
                const internalState = internalStateForModel(
                  snapshot,
                  selectedGroup.provider.id,
                  modelId,
                )
                // multimodalCandidateEligible is backend-resolved across all executable account routes.
                // multimodalCandidateEligible 由后端跨全部可执行账号路由解析。
                const multimodalCandidateEligible = snapshot.mediaUnderstandingModelCandidates?.some(
                  (candidate) => (
                    candidate.selection.provider_id === selectedGroup.provider.id
                    && candidate.selection.model_id === modelId
                  ),
                ) ?? false
                // multimodalCandidateEnabled is one shared provider-model switch, independent from conversation use.
                // multimodalCandidateEnabled 是与会话使用相互独立的共享供应商模型开关。
                const multimodalCandidateEnabled = snapshot.multimodalUnderstandingCandidateModels?.some(
                  (record) => (
                    record.providerId === selectedGroup.provider.id
                    && record.modelId === modelId
                  ),
                ) ?? false
                return (
                  <article className="settings-model-catalog-row" key={`${selectedGroup.provider.id}/${modelId}`}>
                    <div className="settings-model-row-main">
                      <div className="settings-model-title-line">
                        <strong title={displayName}>{displayName}</strong>
                      </div>
                      {/* Keep identity and capability metadata together on the second line.
                          将模型标识和能力摘要放在同一条第二行内。 */}
                      <div className="settings-model-catalog-meta">
                        <span className="settings-model-id" title={modelId}>{modelId}</span>
                        <ProviderModelModalitySummary model={representative} additionalModels={inventory.catalogModels.slice(1)} extensionCapabilities={inventory.extensionCapabilities} />
                        <span>
                          {t("settings.models.availableCredentials")
                            .replace("{count}", String(availableAccountCount))}
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
                            runSnapshotMutation(`conversation/${modelKey}`, () => (
                              savePublicAiModelEnabled({
                                providerId: selectedGroup.provider.id,
                                modelId,
                                enabled: checked,
                              })
                            ))
                          }}
                        />
                      ) : <span title={t("settings.modelExperience.noConversation")} aria-label={t("settings.modelExperience.noConversation")}>—</span>}
                      </div>
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
                              runSnapshotMutation(`multimodal-candidate/${modelKey}`, () => (
                                savePublicAiMultimodalUnderstandingCandidate({
                                  providerId: selectedGroup.provider.id,
                                  modelId,
                                  enabled: checked,
                                })
                              ))
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
                              runSnapshotMutation(`internal/${modelKey}`, () => (
                                savePublicAiInternalModelEnabled({
                                  providerId: selectedGroup.provider.id,
                                  modelId,
                                  enabled: checked,
                                })
                              ))
                            }}
                          />
                        </span></TooltipTrigger><TooltipContent>{t("settings.modelExperience.internalPurposes").replace("{purposes}", internalState.operations.map((operation) => internalOperationLabel(operation, t)).join(t("global.listDelimiter")))}</TooltipContent></Tooltip>
                      ) : null}
                      {inventory.extensionCapabilities.filter(isModelExtensionCapability).map((capability) => {
                        // Matching exact targets preserves the extension's persisted account choice.
                        // 匹配精确目标以保留扩展已持久化的账号选择。
                        const targets = extensionModelTargets(snapshot, selectedGroup.provider.id, modelId, capability)
                        // selections contains the complete saved model set for this extension capability.
                        // selections 包含此扩展能力已保存的完整模型集合。
                        const selections = snapshot.providerExtensionTargetSets[capability] ?? []
                        // selected identifies this model's exact account selection, if present.
                        // selected 标识此模型已选中的精确账号，如有。
                        const selected = selections.find((selection) => targets.some((target) => (
                          target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
                        )))
                        // available is a signed target with the reviewed extension profile required by the backend.
                        // available 是包含后端所需审核扩展资料的签名目标。
                        const available = targets.find((target) => target.target.contract.extension_profile !== undefined)
                        // label names this operation independently from its output resource.
                        // label 独立于输出资源命名此项操作。
                        const label = extensionCapabilityLabel(capability, t)
                        return <Tooltip key={capability}><TooltipTrigger asChild><span className="settings-model-extension-toggle">
                          <Switch
                            aria-label={`${label} ${displayName}`}
                            checked={selected !== undefined}
                            disabled={pendingMutationKey !== null || (!available && !selected)}
                            onCheckedChange={(checked) => {
                              // retained preserves every other model in the capability's complete target set.
                              // retained 保留此能力完整目标集合中的所有其他模型。
                              const retained = selections.filter((selection) => !targets.some((target) => (
                                target.connectionId === selection.connection_id && target.target.target_id === selection.target_id
                              )))
                              if (checked && available) retained.push({ connection_id: available.connectionId, target_id: available.target.target_id })
                              runSnapshotMutation(`extension/${capability}/${modelKey}`, async () => {
                                // next reflects the persisted selection before global capability admission.
                                // next 反映全局能力准入前已经持久化的模型选择。
                                const next = await savePublicAiProviderExtensionTargetSet({ capability, selections: retained })
                                if (checked && !next.providerToolEnabledCapabilities.includes(capability)) {
                                  return savePublicAiProviderToolCapabilityEnabled({ capability, enabled: true })
                                }
                                return next
                              })
                            }}
                          />
                          <small>{label}</small>
                        </span></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
                      })}
                      {!internalState && !inventory.extensionCapabilities.some(isModelExtensionCapability) ? <span>—</span> : null}
                      </div>
                    <div className="settings-model-catalog-actions">
                      {representative ? <Button
                        aria-label={`${t("settings.modelCapabilities.title")} ${displayName}`}
                        disabled={pendingMutationKey !== null}
                        onClick={() => setEditingCapabilityModel(representative)}
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
                          setInspectingModel({ providerId: selectedGroup.provider.id, modelId })
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
          </section>
        )}
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
                    setEditingRoute(route)
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
            model={editingRoute?.model ?? null}
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
              setSnapshot(next)
              setEditingRoute(null)
            }}
            open={editingRoute !== null}
            overrides={snapshot?.modelOverrideRecords.find((record) => (
              record.providerId === selectedGroup?.provider.id
              && record.modelId === editingRoute?.model.id
              && record.protocolId === editingRoute?.protocolId
            ))?.overrides}
            protocolId={editingRoute?.protocolId ?? null}
            providerDisplayName={selectedGroup
              ? localizedProviderName(selectedGroup.provider.id, selectedGroup.provider.display_name, t)
              : null}
            providerId={selectedGroup?.provider.id ?? null}
          />
        </DialogContent>
      </Dialog>
      <PublicAiModelCapabilityDialog
        model={editingCapabilityModel}
        onOpenChange={(open) => {
          if (!open) {
            setEditingCapabilityModel(null)
          }
        }}
        onSaved={(next) => {
          setSnapshot(next)
          setEditingCapabilityModel(null)
        }}
        open={editingCapabilityModel !== null}
        overrides={snapshot?.modelCapabilityOverrideRecords?.find((record) => (
          record.providerId === selectedGroup?.provider.id
          && record.modelId === editingCapabilityModel?.id
        ))?.overrides}
        providerDisplayName={selectedGroup
          ? localizedProviderName(selectedGroup.provider.id, selectedGroup.provider.display_name, t)
          : null}
        providerId={selectedGroup?.provider.id ?? null}
      />
    </WorkspaceDirectoryLayout>
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

// errorMessage converts rejected service values into user-visible diagnostics.
// errorMessage 将被拒绝的服务值转换为用户可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
