import * as React from "react"
import {
  Search,
  SlidersHorizontal,
} from "lucide-react"

import {
  filterProviderModels,
  type FilterableProviderModel,
} from "@/components/features/settings/provider-model-filter"
import { ProviderModelModalitySummary } from "@/components/features/settings/provider-model-modality-summary"
import { providerPriority } from "@/components/features/settings/provider-ordering"
import {
  buildProviderModelGroups,
  compactServiceEntryName,
  type PublicModelProtocolRoute,
} from "@/components/features/settings/provider-model-routes"
import { ProviderIcon } from "@/components/features/settings/provider-icon"
import { localizedProviderName } from "@/components/features/settings/provider-presentation"
import { PublicAiModelOverridesDialog } from "@/components/features/settings/public-ai-model-overrides-dialog"
import { PublicAiModelCapabilityDialog } from "@/components/features/settings/public-ai-model-capability-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { AdaptiveToolPanel } from "@/components/workspace/adaptive-tool-panel"
import { WorkspaceToolPanelMenu } from "@/components/workspace/workspace-tool-panel-menu"
import { useI18n } from "@/i18n"
import {
  loadPublicAiSettings,
  savePublicAiInternalModelEnabled,
  savePublicAiMultimodalUnderstandingCandidate,
  savePublicAiModelEnabled,
} from "@/service/settings-actions-service"
import type {
  PublicAiCodexModelBehavior,
  PublicAiModel,
  PublicAiOperation,
  PublicAiSettingsSnapshot,
} from "@/types"

// PublicProviderModelRow binds one rendered provider model to its routes and effective enabled state.
// PublicProviderModelRow 将一个渲染中的供应商模型绑定到其路由与有效启用状态。
interface PublicProviderModelRow extends FilterableProviderModel {
  // Protocol-scoped routes exposing this model through configured accounts.
  // 通过已配置账号暴露此模型的协议作用域路由。
  routes: PublicModelProtocolRoute[]
}

// ModelListSettingsArea renders the process-local public AI model catalog.
// ModelListSettingsArea 渲染进程内公共 AI 模型目录。
export function ModelListSettingsArea() {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [selectedProviderId, setSelectedProviderId] = React.useState<string | null>(null)
  const [editingRoute, setEditingRoute] = React.useState<PublicModelProtocolRoute | null>(null)
  const [editingCapabilityModel, setEditingCapabilityModel] = React.useState<PublicAiModel | null>(null)
  const [pendingMutationKey, setPendingMutationKey] = React.useState<string | null>(null)
  const [menuExpanded, setMenuExpanded] = React.useState(true)
  const [searchQuery, setSearchQuery] = React.useState("")
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
      .catch((reason) => {
        setError(errorMessage(reason))
      })
      .finally(() => {
        mutationInFlightRef.current = false
        setPendingMutationKey(null)
      })
  }, [])

  React.useEffect(() => {
    let active = true
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
  }, [])

  // groupedProviders keeps credential-backed model providers ordered by persisted priority.
  // groupedProviders 按持久化优先级排列由凭据支持的模型供应商。
  const groupedProviders = React.useMemo(() => {
    if (!snapshot) {
      return []
    }
    // indexedGroups preserves current registry order when priorities are equal.
    // indexedGroups 在优先级相同时保留当前注册表顺序。
    const indexedGroups = buildProviderModelGroups(snapshot)
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
    return Array.from(selectedGroup.models.entries()).flatMap(([modelId, routes]) => {
      const representative = routes[0]
      if (!representative) {
        return []
      }
      const access = snapshot.modelAccessRecords.find((record) => (
        record.providerId === selectedGroup.provider.id
        && record.modelId === modelId
      ))
      return [{
        modelId,
        displayName: representative.model.display_name,
        enabled: access?.enabled ?? true,
        routes,
      }]
    })
  }, [selectedGroup, snapshot])
  // visibleModelRows applies page-local filters without changing catalog or persisted access state.
  // visibleModelRows 应用页面内筛选，不改变目录或持久化访问状态。
  const visibleModelRows = React.useMemo(
    () => filterProviderModels(providerModelRows, searchQuery, "all"),
    [providerModelRows, searchQuery],
  )
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
  // selectedProviderLabel supplies the shared collapsed-menu pill label.
  // selectedProviderLabel 为共享收起菜单胶囊提供标签。
  const selectedProviderLabel = selectedGroup
    ? localizedProviderName(selectedGroup.provider.id, selectedGroup.provider.display_name, t)
    : t("settings.models.noProviderSelected")
  return (
    <AdaptiveToolPanel
      className="workspace-tool-panel-page workspace-skills-settings credential-management-layout model-list-layout"
      contentClassName="workspace-skills-content credential-management-content model-list-content"
      contentShellClassName="credential-management-content-shell"
      expanded={menuExpanded}
      minContentWidth={336}
      panel={(
        <WorkspaceToolPanelMenu
          activeLabel={selectedProviderLabel}
          ariaLabel={t("settings.models.provider")}
          className="credential-management-menu"
          collapseLabel={t("settings.models.collapseProviders")}
          expanded={menuExpanded}
          expandLabel={t("settings.models.expandProviders")}
          heading={t("settings.models.provider")}
          onExpandedChange={setMenuExpanded}
        >
          {groupedProviders.map((group) => (
            <button
              className={`workspace-tool-panel-menu-row ${selectedGroup?.provider.id === group.provider.id ? "active" : ""}`}
              key={group.provider.id}
              onClick={() => setSelectedProviderId(group.provider.id)}
              type="button"
            >
              <ProviderIcon
                category={group.provider.category}
                className="size-5"
                providerId={group.provider.id}
              />
              <span>{localizedProviderName(group.provider.id, group.provider.display_name, t)}</span>
              <small>{group.models.size}</small>
            </button>
          ))}
        </WorkspaceToolPanelMenu>
      )}
      panelWidth={176}
      side="left"
    >
      <div className="settings-direct-content model-list-surface">
        {error ? <p className="settings-muted-line settings-error-text">{error}</p> : null}
        {!snapshot ? (
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
            {blockedModelRoutes.length > 0 ? (
              <section aria-label={t("settings.models.blockedRouteDiagnostics")}>
                <div className="settings-split-column-title">
                  {t("settings.models.blockedRouteDiagnostics")}
                </div>
                <div className="settings-model-row-list">
                  {blockedModelRoutes.map((record) => (
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
                      </div>
                    </article>
                  ))}
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
                </div>
                <div className="settings-model-filter-summary">
                  {t("settings.models.filteredSummary")
                    .replace("{visible}", String(visibleModelRows.length))
                    .replace("{total}", String(providerModelRows.length))}
                </div>
              </div>
              {!selectedGroup || providerModelRows.length === 0 ? (
                <p className="settings-muted-line">{t("settings.models.emptyCatalog")}</p>
              ) : visibleModelRows.length === 0 ? (
                <p className="settings-muted-line">{t("settings.models.emptyFilter")}</p>
              ) : visibleModelRows.map(({ modelId, routes, enabled }) => {
                const representative = routes[0]
                if (!representative) {
                  return null
                }
                const availableAccountCount = routes.reduce(
                  (total, route) => total + route.accountCount,
                  0,
                )
                const modelKey = `${selectedGroup.provider.id}/${modelId}`
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
                  <article className="settings-model-row" key={`${selectedGroup.provider.id}/${modelId}`}>
                    <div className="settings-model-row-main">
                      <div className="settings-model-title-line">
                        <strong>{representative.model.display_name}</strong>
                        <span className="settings-model-id">({modelId})</span>
                        <Badge className="settings-model-credential-count" variant="outline">
                          {t("settings.models.availableCredentials")
                            .replace("{count}", String(availableAccountCount))}
                        </Badge>
                      </div>
                      <Tabs
                        className="settings-model-route-tabs"
                        defaultValue={modelRouteValue(representative)}
                      >
                        <TabsList className="settings-model-route-tabs-list">
                          {routes.map((route) => (
                            <TabsTrigger
                              className="settings-model-route-tab"
                              key={modelRouteValue(route)}
                              title={route.serviceEntryName}
                              value={modelRouteValue(route)}
                            >
                              {compactServiceEntryName(route.serviceEntryName)}
                              {route.accountCount > 1 ? (
                                <Badge variant="secondary">{route.accountCount}</Badge>
                              ) : null}
                            </TabsTrigger>
                          ))}
                        </TabsList>
                        {routes.map((route) => (
                          <TabsContent
                            className="settings-model-route-content"
                            key={modelRouteValue(route)}
                            value={modelRouteValue(route)}
                          >
                            <Button
                              aria-label={t("settings.models.additionalConditions")
                                .replace("{protocol}", protocolDisplayName(route.protocolId))}
                              className="settings-model-route-panel"
                              disabled={pendingMutationKey !== null}
                              onClick={() => setEditingRoute(route)}
                              type="button"
                              variant="ghost"
                            >
                              <span className="settings-model-route-summary">
                                <span className="settings-model-route-protocol">
                                  {protocolDisplayName(route.protocolId)}
                                  <SlidersHorizontal aria-hidden="true" size={14} />
                                </span>
                                <span
                                  className="settings-model-route-binding"
                                  title={`${route.bindingSource} · ${route.bindingRuleId ?? t("settings.models.routeNoRule")} · r${route.manifestRevision}`}
                                >
                                  {route.bindingSource} · {route.bindingRuleId ?? t("settings.models.routeNoRule")} · r{route.manifestRevision}
                                </span>
                                <span
                                  className="settings-model-route-binding"
                                  title={route.availableProtocols.join(", ")}
                                >
                                  {t("settings.models.routeAvailableProtocols")}: {route.availableProtocols.join(", ") || "—"}
                                </span>
                              </span>
                              <ProviderModelModalitySummary model={route.model} />
                            </Button>
                          </TabsContent>
                        ))}
                      </Tabs>
                    </div>
                    <div className="settings-model-switches">
                      <Button
                        aria-label={`${t("settings.modelCapabilities.title")} ${representative.model.display_name}`}
                        onClick={() => setEditingCapabilityModel(representative.model)}
                        size="sm"
                        type="button"
                        variant="outline"
                      >
                        <SlidersHorizontal aria-hidden="true" size={14} />
                        {t("settings.modelCapabilities.shortLabel")}
                      </Button>
                      <label className="settings-model-switch-control">
                        <span>{t("settings.models.conversationUse")}</span>
                        <Switch
                          aria-label={`${t("settings.models.conversationUse")} ${representative.model.display_name}`}
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
                      </label>
                      {multimodalCandidateEligible ? (
                        <label className="settings-model-switch-control">
                          <span>{t("settings.models.multimodalUnderstandingCandidate")}</span>
                          <Switch
                            aria-label={`${t("settings.models.multimodalUnderstandingCandidate")} ${representative.model.display_name}`}
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
                        </label>
                      ) : null}
                      {internalState ? (
                        <label
                          className="settings-model-switch-control"
                          title={internalState.operations
                            .map((operation) => internalOperationLabel(operation, t))
                            .join("、")}
                        >
                          <span>{t("settings.models.internalUse")}</span>
                          <Switch
                            aria-label={`${t("settings.inference.internalToggle")} ${representative.model.display_name}`}
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
                        </label>
                      ) : null}
                    </div>
                  </article>
                )
              })}
            </div>
          </section>
        )}
      <PublicAiModelOverridesDialog
        model={editingRoute?.model ?? null}
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
      </div>
    </AdaptiveToolPanel>
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
