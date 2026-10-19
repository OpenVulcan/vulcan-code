import * as React from "react"
import {
  CircleAlert,
  CircleCheck,
  Check,
  Cpu,
  Database,
  Info,
  LoaderCircle,
  Play,
  Search,
  SlidersHorizontal,
  UserRound,
} from "lucide-react"

import { formatSkillText } from "@/components/features/featureText"
import { PublicAiInternalOverridesDialog } from "@/components/features/settings/public-ai-internal-overrides-dialog"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogTrigger,
} from "@/ui-kit/primitives/dialog"
import { Input } from "@/ui-kit/primitives/input"
import { ScrollArea } from "@/ui-kit/primitives/scroll-area"
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip"
import { useI18n } from "@/i18n"
import { formatDurationSeconds } from "@/lib/usage-format"
import { cn } from "@/lib/utils"
import {
  loadPublicAiSettings,
  savePublicAiInferenceRoute,
  testPublicAiInferenceRoute,
} from "@/service/settings-actions-service"
import { renderServiceErrorMessage } from "@/service/browserTransport"
import type {
  PublicAiInferenceRoute,
  PublicAiInferenceRouteDiagnostic,
  PublicAiInternalInferenceCandidate,
  PublicAiOperation,
  PublicAiSettingsSnapshot,
} from "@/types"
import { errorMessage } from "@/lib/display-format"
import "@/ui-kit/styles/features/settings-forms.css";
import "@/ui-kit/styles/features/memory.css"

// VmmInferenceSettingsProps exposes required-foundation readiness to the first-run workflow.
// VmmInferenceSettingsProps 向首次配置流程暴露必需基础能力就绪状态。
interface VmmInferenceSettingsProps {
  onRequiredRouteReady?: (ready: boolean) => void
  // Called only after the embedding route mutation is durably accepted.
  // 仅在向量路由变更被持久化接受后调用。
  onEmbeddingRouteSaved?: () => void
}

// VMM_REQUIRED_LLM_PURPOSES lists the foundational LLM routes required before VMM can be initialized.
// VMM_REQUIRED_LLM_PURPOSES 列出初始化 VMM 前必须就绪的基础 LLM 路由。
export const VMM_REQUIRED_LLM_PURPOSES = [
  "precheck_l1",
  "precheck_l2",
  "postaction_l1",
  "postaction_l2",
] as const

// vmmRequiredInferenceReady returns whether the exact embedding and foundational LLM routes are executable.
// vmmRequiredInferenceReady 返回精确向量路由和基础 LLM 路由是否均可执行。
export function vmmRequiredInferenceReady(
  snapshot: PublicAiSettingsSnapshot | null,
): boolean {
  if (!snapshot) {
    return false
  }
  const profile = snapshot.inferenceProfiles.vmm
  const embeddingRoute = profile?.embedding
  if (!embeddingRoute) {
    return false
  }
  const candidates = snapshot.internalInferenceCandidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
  ))
  const diagnostics = snapshot.inferenceRouteDiagnostics.filter((diagnostic) => (
    diagnostic.consumerProfileId === "vmm"
  ))
  const embeddingReady = candidates.some((candidate) => (
    candidate.route.operation === "embedding_create"
    && inferenceRouteKey(candidate.route) === inferenceRouteKey(embeddingRoute)
  )) && !diagnostics.some((diagnostic) => (
    diagnostic.operation === "embedding_create"
    && diagnostic.purposeId === undefined
  ))
  if (!embeddingReady) {
    return false
  }
  const llmCandidates = candidates.filter((candidate) => (
    candidate.route.operation === "conversation_respond"
  ))
  return VMM_REQUIRED_LLM_PURPOSES.every((purposeId) => {
    const route = profile.llm_routes?.[purposeId]
    const reasoningEffort = profile.llm_reasoning_efforts?.[purposeId] ?? "none"
    return Boolean(
      route
      && reasoningEffort === "none"
      && llmCandidates.some((candidate) => (
        inferenceRouteKey(candidate.route) === inferenceRouteKey(route)
        && candidateSupportsDisabledReasoning(candidate)
        && candidateSupportsVmmStructuredOutput(candidate)
      ))
      && !diagnostics.some((diagnostic) => (
        diagnostic.operation === "conversation_respond"
        && diagnostic.purposeId === purposeId
      )),
    )
  })
}

// VmmInferenceSettings owns VMM inference route loading, mutation, and validation.
// VmmInferenceSettings 管理 VMM 推理路由的加载、变更与验证。
export function VmmInferenceSettings({
  onRequiredRouteReady,
  onEmbeddingRouteSaved,
}: VmmInferenceSettingsProps = {}) {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  // loadRevision permits an explicit retry without remounting or changing persisted routes.
  // loadRevision 允许明确重试，不重新挂载或改变持久化路由。
  const [loadRevision, setLoadRevision] = React.useState(0)
  // loadPending distinguishes an unfinished read from a settled catalog failure.
  // loadPending 区分尚未完成的读取和已结束的目录加载失败。
  const [loadPending, setLoadPending] = React.useState(true)
  const [editingCandidate, setEditingCandidate] =
    React.useState<PublicAiInternalInferenceCandidate | null>(null)
  const [testingOperation, setTestingOperation] = React.useState<PublicAiOperation | null>(null)
  const [testSummaries, setTestSummaries] =
    React.useState<Partial<Record<PublicAiOperation, string>>>({})
  const [pendingMutationKey, setPendingMutationKey] = React.useState<string | null>(null)
  const mutationInFlightRef = React.useRef(false)

  React.useEffect(() => {
    let active = true
    setLoadPending(true)
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
      .finally(() => { if (active) setLoadPending(false) })
    return () => {
      active = false
    }
  }, [loadRevision])

  // runSnapshotMutation serializes authoritative inference-profile replacements.
  // runSnapshotMutation 串行化权威推理配置档替换。
  const runSnapshotMutation = React.useCallback((
    key: string,
    mutation: () => Promise<PublicAiSettingsSnapshot>,
  ): Promise<boolean> => {
    if (mutationInFlightRef.current) {
      return Promise.resolve(false)
    }
    mutationInFlightRef.current = true
    setPendingMutationKey(key)
    return mutation()
      .then(async (next) => {
        setSnapshot(next)
        setError(null)
        return true
      })
      .catch((reason) => {
        setError(errorMessage(reason))
        return false
      })
      .finally(() => {
        mutationInFlightRef.current = false
        setPendingMutationKey(null)
      })
  }, [])

  const profile = snapshot?.inferenceProfiles.vmm
  // Native VMM executes as host-owned background work, so other scopes are not selectable.
  // 原生 VMM 作为宿主持有的后台工作执行，因此其他用途范围不可选择。
  const candidates = snapshot?.internalInferenceCandidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "internal_background"
  )) ?? []
  // llmCandidates retain only routes that preserve both disabled reasoning and structured JSON.
  // llmCandidates 仅保留同时保持关闭思考与结构化 JSON 的路由。
  const llmCandidates = candidates.filter((candidate) => (
    candidate.route.operation === "conversation_respond"
    && candidateSupportsDisabledReasoning(candidate)
    && candidateSupportsVmmStructuredOutput(candidate)
  ))
  const diagnostics = snapshot?.inferenceRouteDiagnostics.filter((diagnostic) => (
    diagnostic.consumerProfileId === "vmm"
  )) ?? []
  // requiredRoutesReady gates first initialization on embedding plus all PreCheck/PostAction LLM routes.
  // requiredRoutesReady 使用向量路由和全部 PreCheck/PostAction LLM 路由共同门控首次初始化。
  const requiredRoutesReady = vmmRequiredInferenceReady(snapshot)
  // routesDisabled prevents settings writes while a route mutation or connection test owns the workflow.
  // routesDisabled 在路由变更或连接测试占用流程时阻止设置写入。
  const routesDisabled = pendingMutationKey !== null || testingOperation !== null

  React.useEffect(() => {
    onRequiredRouteReady?.(requiredRoutesReady)
  }, [onRequiredRouteReady, requiredRoutesReady])
  const purposes: Array<{ id: string; label: string; description: string }> = [
    {
      id: "precheck_l1",
      label: t("settings.inference.purpose.precheckL1"),
      description: t("settings.inference.purpose.precheckL1Description"),
    },
    {
      id: "precheck_l2",
      label: t("settings.inference.purpose.precheckL2"),
      description: t("settings.inference.purpose.precheckL2Description"),
    },
    {
      id: "postaction_l1",
      label: t("settings.inference.purpose.postactionL1"),
      description: t("settings.inference.purpose.postactionL1Description"),
    },
    {
      id: "postaction_l2",
      label: t("settings.inference.purpose.postactionL2"),
      description: t("settings.inference.purpose.postactionL2Description"),
    },
    {
      id: "profile_instruction",
      label: t("settings.inference.purpose.profileInstruction"),
      description: t("settings.inference.purpose.profileInstructionDescription"),
    },
    {
      id: "reserve",
      label: t("settings.inference.purpose.reserve"),
      description: t("settings.inference.purpose.reserveDescription"),
    },
  ]
  const matchingEditingCandidates = React.useMemo(() => {
    if (!editingCandidate || !snapshot) {
      return []
    }
    return snapshot.internalInferenceCandidates.filter((candidate) => (
      candidate.route.provider_id === editingCandidate.route.provider_id
      && candidate.route.model_id === editingCandidate.route.model_id
      && candidate.route.operation === editingCandidate.route.operation
    ))
  }, [editingCandidate, snapshot])

  // testRoute executes one real provider request and retains only compact verified facts.
  // testRoute 执行一次真实供应商请求，并仅保留紧凑的已验证事实。
  const testRoute = React.useCallback(async (
    operation: "embedding_create" | "rerank_documents",
  ) => {
    setTestingOperation(operation)
    setError(null)
    setTestSummaries((current) => ({ ...current, [operation]: undefined }))
    try {
      const result = await testPublicAiInferenceRoute({
        consumerProfileId: "vmm",
        operation,
      })
      const summary = operation === "embedding_create"
        ? formatSkillText(t("settings.inference.testEmbeddingSuccess"), {
            count: result.resultCount,
            dimensions: result.dimensions ?? 0,
            duration: formatDurationSeconds(result.durationMs),
          })
        : formatSkillText(t("settings.inference.testRerankSuccess"), {
            count: result.resultCount,
            duration: formatDurationSeconds(result.durationMs),
          })
      setTestSummaries((current) => ({ ...current, [operation]: summary }))
    } catch (cause) {
      setError(errorMessage(cause))
    } finally {
      setTestingOperation(null)
    }
  }, [t])

  // saveRoute persists one exact VMM operation route and refreshes the full snapshot.
  // saveRoute 持久化一条精确 VMM 操作路由，并刷新完整快照。
  const saveRoute = React.useCallback((
    operation: PublicAiOperation,
    purposeId: string | undefined,
    route: PublicAiInferenceRoute | null,
  ) => {
    const key = `${operation}/${purposeId ?? "default"}`
    setTestSummaries((current) => ({ ...current, [operation]: undefined }))
    return runSnapshotMutation(
      key,
      () => savePublicAiInferenceRoute({
        consumerProfileId: "vmm",
        purposeId,
        operation,
        route,
        reasoningEffort: operation === "conversation_respond" && route ? "none" : undefined,
      }),
    ).then((saved) => {
      // Model connectivity can remain green while a newly saved route invalidates stored vectors.
      // 模型连通性仍可能显示绿色，但新保存的路由会使已有向量失效。
      if (saved && operation === "embedding_create") onEmbeddingRouteSaved?.()
      return saved
    })
  }, [onEmbeddingRouteSaved, runSnapshotMutation])

  return (
    <section className="vk-form-box">
      <div className="vk-mem-section-head">
        <Database aria-hidden="true" />
        <div className="vk-form-cell">
          <div className="vk-form-inline"><strong>{t("settings.inference.title")} · {t("settings.inference.profileVmm")}</strong><Tooltip><TooltipTrigger asChild><button type="button" aria-label={t("settings.inference.vmmReasoningNotice")} className="vk-faint"><Info className="vk-icon" aria-hidden="true" /></button></TooltipTrigger><TooltipContent>{t("settings.inference.vmmReasoningNotice")}</TooltipContent></Tooltip></div>
          <p className="vk-form-note">{t("settings.inference.subtitle")}</p>
        </div>
      </div>
      {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
      {!snapshot ? (
        loadPending ? <StreamPlaceholder label={t("settings.models.loadingCatalog")} shape="fields" count={3} />
          : <Button type="button" variant="outline" onClick={() => setLoadRevision((current) => current + 1)}>{t("settings.modelExperience.retryLoad")}</Button>
      ) : (
        <>
          <div className="vmm-inference-routes">
            <div className="vmm-inference-columns vk-faint vk-form-xs" aria-hidden="true">
              <span>{t("memory.workspace.purpose")}</span>
              <div className="vmm-inference-controls"><span>{t("settings.inference.modelRegion")}</span><span>{t("settings.inference.accountRegion")}</span></div>
              <span className="vk-mem-end-text">{t("memory.workspace.actions")}</span>
            </div>
            {purposes.map((purpose) => (
              <InferenceRouteField
                candidates={llmCandidates}
                disabled={routesDisabled}
                description={purpose.description}
                key={purpose.id}
                label={purpose.label}
                onConfigure={setEditingCandidate}
                onChange={(route) => saveRoute(
                  "conversation_respond",
                  purpose.id,
                  route,
                )}
                route={profile?.llm_routes?.[purpose.id]}
                unavailableReason={diagnosticReason(diagnostics.find((diagnostic) => (
                  diagnostic.operation === "conversation_respond"
                  && diagnostic.purposeId === purpose.id
                )))}
              />
            ))}
            <InferenceRouteField
              candidates={candidates.filter((candidate) => (
                candidate.route.operation === "embedding_create"
              ))}
              disabled={routesDisabled}
              label={t("settings.inference.embedding")}
              onConfigure={setEditingCandidate}
              onChange={(route) => saveRoute("embedding_create", undefined, route)}
              onTest={() => void testRoute("embedding_create")}
              route={profile?.embedding}
              testPending={testingOperation === "embedding_create"}
              testSummary={testSummaries.embedding_create}
              unavailableReason={diagnosticReason(diagnostics.find((diagnostic) => (
                diagnostic.operation === "embedding_create"
                && diagnostic.purposeId === undefined
              )))}
            />
            <InferenceRouteField
              candidates={candidates.filter((candidate) => (
                candidate.route.operation === "rerank_documents"
              ))}
              disabled={routesDisabled}
              label={t("settings.inference.rerank")}
              onConfigure={setEditingCandidate}
              onChange={(route) => saveRoute("rerank_documents", undefined, route)}
              onTest={() => void testRoute("rerank_documents")}
              route={profile?.rerank}
              showEnableToggle
              testPending={testingOperation === "rerank_documents"}
              testSummary={testSummaries.rerank_documents}
              unavailableReason={diagnosticReason(diagnostics.find((diagnostic) => (
                diagnostic.operation === "rerank_documents"
                && diagnostic.purposeId === undefined
              )))}
            />
          </div>
          <PublicAiInternalOverridesDialog
            candidate={editingCandidate}
            matchingCandidates={matchingEditingCandidates}
            onOpenChange={(open) => {
              if (!open) {
                setEditingCandidate(null)
              }
            }}
            onSaved={(next) => {
              setSnapshot(next)
              setError(null)
            }}
            open={editingCandidate !== null}
          />
        </>
      )}
    </section>
  )
}

// InferencePickerOption is one searchable row in the shared route picker surface.
// InferencePickerOption 是共享路由选择界面中的一条可搜索记录。
interface InferencePickerOption {
  // Stable value emitted when this option is chosen.
  // 选择该项时发出的稳定值。
  key: string
  // Primary user-visible label.
  // 面向用户的主标签。
  title: string
  // Secondary provider, account, or service-entry label.
  // 次级供应商、账号或服务入口标签。
  subtitle: string
  // Right-aligned model, protocol, or binding identity.
  // 右对齐的模型、协议或绑定身份。
  trailing: string
  // Complete normalized search source.
  // 完整的标准化搜索来源。
  searchText: string
}

// InferenceRouteField renders one route selector from exact backend candidates.
// InferenceRouteField 根据精确后端候选渲染一个路由选择器。
function InferenceRouteField(props: {
  // User-visible route purpose.
  // 面向用户的路由用途。
  label: string
  // Human explanation of the internal task served by this route.
  // 该路由所服务内部任务的人类可读说明。
  description?: string
  // Persisted exact route.
  // 已持久化的精确路由。
  route?: PublicAiInferenceRoute
  // Catalog-backed candidates supplied by the backend.
  // 后端提供的目录候选。
  candidates: PublicAiInternalInferenceCandidate[]
  // Whether mutation or testing prevents interaction.
  // 变更或测试是否阻止交互。
  disabled: boolean
  // Whether this route exposes the explicit enabled checkbox used by optional capabilities.
  // 该路由是否展示可选能力使用的独立启用勾选项。
  showEnableToggle?: boolean
  // Receives an exact route replacement or clear request.
  // 接收精确路由替换或清除请求。
  onChange: (route: PublicAiInferenceRoute | null) => void | Promise<boolean>
  // Optionally confirms a route change before local and persisted state changes.
  // 可选地在本地状态和持久化状态变更前确认路由变更。
  onBeforeChange?: (route: PublicAiInferenceRoute | null) => Promise<boolean>
  // Opens operation-scoped defaults for one selected route.
  // 打开所选路由的操作作用域默认值。
  onConfigure: (candidate: PublicAiInternalInferenceCandidate) => void
  // Exact orphaned-route diagnostic.
  // 精确的孤立路由诊断。
  unavailableReason?: string
  // Starts a provider-backed route test.
  // 启动供应商支持的路由测试。
  onTest?: () => void
  // Whether the provider-backed route test is running.
  // 供应商支持的路由测试是否正在运行。
  testPending?: boolean
  // Latest compact verified test result.
  // 最近一次紧凑的已验证测试结果。
  testSummary?: string
}) {
  const { t } = useI18n()
  // persistedRouteKey tracks authoritative route changes without resetting a local model draft.
  // persistedRouteKey 跟踪权威路由变化且不会重置本地模型草稿。
  const persistedRouteKey = props.route ? inferenceRouteKey(props.route) : ""
  // persistedModelKey removes account identity from the authoritative route.
  // persistedModelKey 从权威路由中移除账号身份。
  const persistedModelKey = props.route ? inferenceModelKey(props.route) : null
  // selectedModelKey is the model-first draft used to populate the separate account picker.
  // selectedModelKey 是先选模型后填充独立账号选择器的草稿。
  const [selectedModelKey, setSelectedModelKey] = React.useState<string | null>(persistedModelKey)
  // enabledOverride keeps a newly enabled but not-yet-configured rerank route selectable before persistence.
  // enabledOverride 让刚启用但尚未持久化配置的重排路由在保存前保持可选择。
  const [enabledOverride, setEnabledOverride] = React.useState<boolean | null>(null)
  // featureEnabled combines the persisted route with the local first-enable draft.
  // featureEnabled 将已持久化路由与首次启用时的本地草稿合并为当前开关状态。
  const featureEnabled = props.showEnableToggle
    ? (enabledOverride ?? Boolean(props.route))
    : true
  // candidateByRouteKey resolves one exact account route after the second selection step.
  // candidateByRouteKey 在第二步选择后解析一条精确账号路由。
  const candidateByRouteKey = React.useMemo(() => new Map(
    props.candidates.map((candidate) => [inferenceRouteKey(candidate.route), candidate]),
  ), [props.candidates])
  // modelOptions collapse credential duplicates while retaining provider/model identity.
  // modelOptions 折叠凭据重复项，同时保留供应商与模型身份。
  const modelOptions = React.useMemo(() => {
    const options = new Map<string, InferencePickerOption>()
    for (const candidate of props.candidates) {
      const key = inferenceModelKey(candidate.route)
      if (!options.has(key)) {
        options.set(key, {
          key,
          title: candidate.modelDisplayName,
          subtitle: candidate.providerDisplayName,
          trailing: candidate.route.model_id,
          searchText: [
            candidate.modelDisplayName,
            candidate.providerDisplayName,
            candidate.route.model_id,
          ].join(" "),
        })
      }
    }
    return Array.from(options.values())
  }, [props.candidates])
  // accountCandidates contain only exact credentials for the model chosen in the first region.
  // accountCandidates 只包含第一块已选模型对应的精确凭据。
  const accountCandidates = React.useMemo(() => props.candidates.filter((candidate) => (
    selectedModelKey !== null && inferenceModelKey(candidate.route) === selectedModelKey
  )), [props.candidates, selectedModelKey])
  // accountOptions retain service-entry and protocol facts owned by each credential route.
  // accountOptions 保留每条凭据路由拥有的服务入口与协议事实。
  const accountOptions = React.useMemo(() => accountCandidates.map((candidate) => ({
    key: inferenceRouteKey(candidate.route),
    title: candidate.connectionDisplayName,
    subtitle: candidate.serviceEntryDisplayName,
    trailing: inferenceBindingLabel(candidate.route),
    searchText: [
      candidate.connectionDisplayName,
      candidate.serviceEntryDisplayName,
      inferenceBindingLabel(candidate.route),
    ].join(" "),
  })), [accountCandidates])
  // selectedRouteBelongsToDraft prevents an old account from appearing under a newly drafted model.
  // selectedRouteBelongsToDraft 防止旧账号显示在新选择的模型之下。
  const selectedRouteBelongsToDraft = Boolean(
    props.route && selectedModelKey === inferenceModelKey(props.route),
  )
  // selectedAccountKey identifies the persisted account only when its model still matches.
  // selectedAccountKey 仅在模型仍匹配时标识已持久化账号。
  const selectedAccountKey = selectedRouteBelongsToDraft ? persistedRouteKey : null
  // selectedCandidate owns the configure and test actions for the exact persisted route.
  // selectedCandidate 为精确持久化路由提供配置与测试动作。
  const selectedCandidate = selectedAccountKey
    ? candidateByRouteKey.get(selectedAccountKey)
    : undefined

  // requestRouteChange confirms before committing the local model draft and exact route.
  // requestRouteChange 在提交本地模型草稿和精确路由前执行确认。
  const requestRouteChange = React.useCallback(async (
    route: PublicAiInferenceRoute | null,
    nextModelKey: string | null,
  ): Promise<boolean> => {
    // nextRouteKey identifies the exact provider/account route before any confirmation or write.
    // nextRouteKey 在确认或写入前标识精确的提供商/账户路由。
    const nextRouteKey = route ? inferenceRouteKey(route) : ""
    if (persistedRouteKey === nextRouteKey) {
      setSelectedModelKey(nextModelKey)
      return true
    }
    if (props.onBeforeChange && !await props.onBeforeChange(route)) {
      return false
    }
    setSelectedModelKey(nextModelKey)
    // persisted is the asynchronous route-write outcome used to restore the previous draft on failure.
    // persisted 是异步路由写入结果，用于失败时恢复之前的草稿。
    const persisted = props.onChange(route)
    if (persisted instanceof Promise && !await persisted) {
      setSelectedModelKey(persistedModelKey)
      return false
    }
    return true
  }, [
    candidateByRouteKey,
    persistedModelKey,
    persistedRouteKey,
    props.onBeforeChange,
    props.onChange,
  ])

  // changeEnabledState opens selection for a new route or clears the persisted route when disabled.
  // changeEnabledState 在新路由启用时打开选择，在关闭时清除已持久化路由。
  const changeEnabledState = React.useCallback(async (enabled: boolean) => {
    if (!props.showEnableToggle) {
      return
    }
    if (enabled) {
      setEnabledOverride(true)
      return
    }
    setEnabledOverride(false)
    if (!await requestRouteChange(null, null)) {
      setEnabledOverride(null)
    }
  }, [props.showEnableToggle, requestRouteChange])

  React.useEffect(() => {
    setSelectedModelKey(persistedModelKey)
    setEnabledOverride(null)
  }, [persistedRouteKey])

  return (
    <div className="vmm-inference-route">
      <div className="vmm-inference-heading vk-form-bar">
        <div className="vk-form-inline">
          <label className="vk-field-label">{props.label}</label>
          {props.description ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  aria-label={props.description}
                  className="vk-mem-info"
                  type="button"
                >
                  <Info className="vk-icon" aria-hidden="true" />
                </button>
              </TooltipTrigger>
              <TooltipContent className="vk-mem-tip" side="top">
                {props.description}
              </TooltipContent>
            </Tooltip>
          ) : null}
        </div>
        {props.showEnableToggle ? (
          <label className="vk-check-label vk-form-note">
            <Checkbox
              aria-label={t("settings.inference.rerankEnabled")}
              checked={featureEnabled}
              disabled={props.disabled || (!featureEnabled && props.candidates.length === 0)}
              onCheckedChange={(checked) => {
                void changeEnabledState(checked === true)
              }}
            />
            <span>{t("settings.inference.rerankEnabled")}</span>
          </label>
        ) : null}
      </div>
      <div className="vmm-inference-controls">
        <div className="vk-form-cell">
          <span className="vmm-inference-mobile-label vk-form-note">
            {t("settings.inference.modelRegion")}
          </span>
          <InferencePickerControl
            ariaLabel={`${props.label} · ${t("settings.inference.modelRegion")}`}
            disabled={props.disabled || !featureEnabled}
            emptyLabel={t("composer.modelPicker.noMatches")}
            filterPlaceholder={t("composer.modelPicker.filter")}
            icon={<Cpu className="vk-icon" />}
            onValueChange={(value) => {
              if (value === null) {
                void requestRouteChange(null, null)
                return
              }
              if (props.route && value === persistedModelKey) {
                setSelectedModelKey(value)
                return
              }
              // firstAvailableAccount immediately applies the first account for a newly selected model.
              // firstAvailableAccount 会在选择新模型时立即应用第一个账号。
              const firstAvailableAccount = props.candidates.find((candidate) => (
                inferenceModelKey(candidate.route) === value
              ))
              if (firstAvailableAccount) {
                void requestRouteChange(
                  firstAvailableAccount.route,
                  value,
                )
              } else {
                setSelectedModelKey(value)
              }
            }}
            options={modelOptions}
            placeholder={t("settings.inference.selectModel")}
            selectedFallback={props.route?.model_id}
            showNoRoute={!props.showEnableToggle}
            value={selectedModelKey}
          />
        </div>
        <div className="vk-form-cell">
          <span className="vmm-inference-mobile-label vk-form-note">
            {t("settings.inference.accountRegion")}
          </span>
          <div className="vmm-inference-account">
          <InferencePickerControl
            ariaLabel={`${props.label} · ${t("settings.inference.accountRegion")}`}
            disabled={props.disabled || !featureEnabled || selectedModelKey === null}
            emptyLabel={t("settings.inference.noAccountMatches")}
            filterPlaceholder={t("settings.inference.accountFilter")}
            icon={<UserRound className="vk-icon" />}
            onValueChange={(value) => {
              void requestRouteChange(
                value ? candidateByRouteKey.get(value)?.route ?? null : null,
                selectedModelKey,
              )
            }}
            options={accountOptions}
            placeholder={selectedModelKey
              ? t("settings.inference.selectAccount")
              : t("settings.inference.selectModelFirst")}
            selectedFallback={props.route?.connection_id}
            showNoRoute={!props.showEnableToggle}
            value={selectedAccountKey}
          />
          <InferenceRouteStatus
            candidate={selectedCandidate}
            enabled={featureEnabled}
            routeConfigured={Boolean(props.route)}
            testSummary={props.testSummary}
            unavailableReason={props.unavailableReason}
          />
          </div>
        </div>
      </div>
      {props.route && !selectedCandidate ? <p className="vk-field-error" role="status">{t("settings.inference.unavailable")} · {t("settings.inference.selectAccount")}</p> : null}
      <div className="vmm-inference-actions vk-form-acts">
        <Button
          aria-label={t("settings.inference.configureDefaults")}
                    disabled={props.disabled || !featureEnabled || !selectedCandidate}
          onClick={() => {
            if (selectedCandidate) {
              props.onConfigure(selectedCandidate)
            }
          }}
          size="icon"
          title={t("settings.inference.configureDefaults")}
          type="button"
          variant="outline"
        >
          <SlidersHorizontal className="vk-icon" />
        </Button>
        {props.onTest ? (
          <Button
            aria-label={t("settings.inference.testConnection")}
                        disabled={props.disabled || !featureEnabled || props.testPending || !selectedCandidate}
            onClick={props.onTest}
            size="icon"
            title={t("settings.inference.testConnection")}
            type="button"
            variant="outline"
          >
            {props.testPending ? (
              <LoaderCircle className="vk-spin" />
            ) : (
              <Play className="vk-icon" />
            )}
          </Button>
        ) : null}
      </div>
    </div>
  )
}

// InferenceRouteStatus renders a focusable status icon from authoritative route availability.
// InferenceRouteStatus 根据权威路由可用性渲染可聚焦的状态图标。
// Parameters identify the selected credential, enabled state, diagnostic and explicit test result.
// 参数标识已选凭据、启用状态、诊断信息与明确的测试结果。
// Returns a tooltip trigger; availability alone never claims a successful live connection test.
// 返回提示触发器；仅凭可用性不会声称真实连接测试已经成功。
function InferenceRouteStatus(props: {
  // Exact selected credential, if it remains in the current catalog.
  // 当前目录中仍然存在的精确已选凭据。
  candidate: PublicAiInternalInferenceCandidate | undefined
  // Whether the operation is enabled.
  // 当前操作是否启用。
  enabled: boolean
  // Whether a route has been saved for this operation.
  // 当前操作是否已保存路由。
  routeConfigured: boolean
  // Result of an explicitly requested live test.
  // 明确请求的真实测试结果。
  testSummary: string | undefined
  // Authoritative diagnostic for this exact route.
  // 此精确路由的权威诊断。
  unavailableReason: string | undefined
}) {
  const { t } = useI18n()
  // failed prioritizes diagnostics over a candidate that may still exist in the catalog.
  // failed 优先采用诊断，避免目录中仍存在的候选遮蔽错误。
  const failed = props.enabled && Boolean(props.unavailableReason || (props.routeConfigured && !props.candidate))
  // available means the configured credential is selectable, not that a live test ran.
  // available 表示配置凭据可选，不表示已经执行真实测试。
  const available = props.enabled && !failed && Boolean(props.candidate)
  // label uses the explicit backend diagnostic when available and distinguishes disabled routes.
  // label 优先使用明确后端诊断，并区分未启用路由。
  const label = !props.enabled ? t("settings.inference.rerankDisabled")
    : props.unavailableReason ? formatSkillText(t("settings.inference.unavailableReason"), { reason: props.unavailableReason })
    : failed ? t("settings.inference.unavailable")
    : available ? t("settings.inference.credentialAvailable") : t("settings.inference.noRoute")
  return <Tooltip><TooltipTrigger asChild><button type="button" aria-label={label} className="vmm-inference-status" data-tone={failed ? "bad" : available ? "ok" : undefined}>
    {failed ? <CircleAlert className="vk-icon" aria-hidden="true" /> : available ? <CircleCheck className="vk-icon" aria-hidden="true" /> : <Info className="vk-icon" aria-hidden="true" />}
  </button></TooltipTrigger><TooltipContent><span>{label}{props.candidate ? <span className="vk-mem-tip-line">{props.candidate.serviceEntryDisplayName}</span> : null}{props.testSummary ? <span className="vk-mem-tip-line">{props.testSummary}</span> : null}</span></TooltipContent></Tooltip>
}

// InferencePickerControl renders the existing searchable model-picker visual language.
// InferencePickerControl 渲染现有的可搜索模型选择器视觉语言。
function InferencePickerControl(props: {
  // Accessible trigger and menu label.
  // 无障碍触发器与菜单标签。
  ariaLabel: string
  // Current stable option identity.
  // 当前稳定选项身份。
  value: string | null
  // Searchable choices for this picker stage.
  // 当前选择阶段的可搜索选项。
  options: InferencePickerOption[]
  // Trigger copy when no option is selected.
  // 未选择选项时的触发器文案。
  placeholder: string
  // Search field placeholder.
  // 搜索字段占位文案。
  filterPlaceholder: string
  // Empty-state copy after filtering.
  // 过滤后空状态文案。
  emptyLabel: string
  // Optional label for an unavailable persisted value.
  // 已失效持久化值的可选标签。
  selectedFallback?: string
  // Leading trigger icon.
  // 触发器前置图标。
  icon: React.ReactNode
  // Whether the picker is temporarily unavailable.
  // 选择器是否暂时不可用。
  disabled: boolean
  // Whether the explicit no-route option remains visible in this picker.
  // 该选择器是否继续显示显式的未配置选项。
  showNoRoute?: boolean
  // Receives one selected identity or an explicit clear action.
  // 接收一个已选身份或显式清除动作。
  onValueChange: (value: string | null) => void
}) {
  const { t } = useI18n()
  // open controls the shared searchable picker surface.
  // open 控制共享的可搜索选择器界面。
  const [open, setOpen] = React.useState(false)
  // filter stores the current visible search query.
  // filter 存储当前可见搜索条件。
  const [filter, setFilter] = React.useState("")
  // normalizedFilter supports case-insensitive matching without changing identities.
  // normalizedFilter 支持不改变身份的大小写无关匹配。
  const normalizedFilter = filter.trim().toLocaleLowerCase()
  // selectedOption resolves only an exact current identity.
  // selectedOption 只解析精确的当前身份。
  const selectedOption = props.options.find((option) => option.key === props.value)
  // filteredOptions retain caller ordering after applying one bounded query.
  // filteredOptions 在应用一个有限查询后保留调用方排序。
  const filteredOptions = React.useMemo(() => {
    if (!normalizedFilter) {
      return props.options
    }
    return props.options.filter((option) => (
      option.searchText.toLocaleLowerCase().includes(normalizedFilter)
    ))
  }, [normalizedFilter, props.options])
  // triggerLabel never exposes encoded persistence identities.
  // triggerLabel 永不暴露编码后的持久化身份。
  const triggerLabel = selectedOption?.title
    ?? props.selectedFallback
    ?? props.placeholder

  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen)
        if (!nextOpen) {
          setFilter("")
        }
      }}
      open={open}
    >
      <DialogTrigger asChild>
        <Button
          aria-label={props.ariaLabel}
          className="vk-mem-picker-trigger"
          disabled={props.disabled}
          type="button"
          variant="outline"
        >
          <span className="vk-faint">{props.icon}</span>
          <span>{triggerLabel}</span>
        </Button>
      </DialogTrigger>
      <DialogContent className="vmm-model-dialog" size="lg">
        <DialogHeader><DialogTitle>{props.ariaLabel}</DialogTitle><DialogDescription>{props.filterPlaceholder}</DialogDescription></DialogHeader>
        <div className="model-picker-filter">
          <Search size={14} />
          <Input
            aria-label={props.filterPlaceholder}
            autoFocus
            onChange={(event) => setFilter(event.currentTarget.value)}
            placeholder={props.filterPlaceholder}
            value={filter}
          />
        </div>
        <ScrollArea className="model-picker-scroll">
          <div className="model-picker-list">
            {props.showNoRoute !== false ? (
                <Button
                  className={cn("model-picker-row", props.value === null && "selected")}
                  aria-pressed={props.value === null}
                  onClick={() => {
                    props.onValueChange(null)
                    setOpen(false)
                  }}
                  type="button"
                  variant="ghost"
                >
                  <span className="model-picker-selected-slot">
                    {props.value === null ? <Check size={15} /> : null}
                  </span>
                  <span className="model-picker-row-copy">
                    <span className="model-picker-row-title">
                      {t("settings.inference.noRoute")}
                    </span>
                  </span>
                </Button>
            ) : null}
            {filteredOptions.length === 0 ? (
              <div className="model-picker-empty">{props.emptyLabel}</div>
            ) : (
              filteredOptions.map((option) => {
                // selected marks the exact model or account identity for this picker stage.
                // selected 标记当前选择阶段的精确模型或账号身份。
                const selected = option.key === props.value
                return (
                    <Button
                      key={option.key}
                      className={cn("model-picker-row", selected && "selected")}
                      aria-pressed={selected}
                      onClick={() => {
                        props.onValueChange(option.key)
                        setOpen(false)
                      }}
                      type="button"
                      variant="ghost"
                    >
                      <span className="model-picker-selected-slot">
                        {selected ? <Check size={15} /> : null}
                      </span>
                      <span className="model-picker-row-copy">
                        <span className="model-picker-row-title">{option.title}</span>
                        <span className="model-picker-row-meta">{option.subtitle}</span>
                      </span>
                    <span className="model-picker-row-id">{option.trailing}</span>
                    </Button>
                )
              })
            )}
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  )
}

// inferenceModelKey removes account and protocol identity from one exact route.
// inferenceModelKey 从一条精确路由中移除账号与协议身份。
function inferenceModelKey(route: PublicAiInferenceRoute): string {
  return [route.provider_id, route.model_id].join("\u0000")
}

// inferenceBindingLabel exposes the protocol or native action owned by one account route.
// inferenceBindingLabel 展示一条账号路由拥有的协议或原生动作。
function inferenceBindingLabel(route: PublicAiInferenceRoute): string {
  return route.binding.kind === "protocol"
    ? route.binding.protocol_profile_id
    : route.binding.action_binding_id
}

// inferenceRouteKey creates a deterministic route identity without parsing persistence keys.
// inferenceRouteKey 创建确定性路由身份且不解析持久化键。
function inferenceRouteKey(route: PublicAiInferenceRoute): string {
  const binding = route.binding.kind === "protocol"
    ? `protocol:${route.binding.protocol_profile_id}`
    : `action:${route.binding.action_binding_id}`
  return [
    route.provider_id,
    route.service_entry_id,
    route.connection_id,
    route.model_id,
    binding,
    route.operation,
    route.usage_scope,
  ].join("\u0000")
}

// candidateSupportsDisabledReasoning mirrors the backend's strict VMM eligibility contract.
// candidateSupportsDisabledReasoning 镜像后端严格的 VMM 候选资格契约。
function candidateSupportsDisabledReasoning(
  candidate: PublicAiInternalInferenceCandidate,
): boolean {
  const control = candidate.capabilities.reasoning_control ?? "unknown"
  if (control === "fixed_off") {
    return true
  }
  if (
    control !== "toggle"
    && control !== "effort_selection"
    && control !== "provider_adaptive"
  ) {
    return false
  }
  return (candidate.capabilities.reasoning_efforts ?? []).includes("none")
}

// candidateSupportsVmmStructuredOutput consumes the backend-projected model/protocol intersection.
// candidateSupportsVmmStructuredOutput 消费后端投影后的模型与协议能力交集。
function candidateSupportsVmmStructuredOutput(
  candidate: PublicAiInternalInferenceCandidate,
): boolean {
  return candidate.capabilities.structured_output === "json_object"
    || candidate.capabilities.structured_output === "json_schema"
}

// diagnosticReason renders a route failure in the active language when the service sent a catalog cause.
// diagnosticReason 在服务给出目录原因时以当前语言渲染路由失败原因。
function diagnosticReason(diagnostic: PublicAiInferenceRouteDiagnostic | undefined): string | undefined {
  if (!diagnostic) return undefined
  return renderServiceErrorMessage(diagnostic.reason, diagnostic.reasonMessage ?? undefined)
}
