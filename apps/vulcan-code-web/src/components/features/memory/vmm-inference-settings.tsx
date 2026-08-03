import * as React from "react"
import {
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
import { Button } from "@/components/ui/button"
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import { cn } from "@/lib/utils"
import {
  loadPublicAiSettings,
  savePublicAiInferenceRoute,
  testPublicAiInferenceRoute,
} from "@/service/settings-actions-service"
import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiOperation,
  PublicAiSettingsSnapshot,
} from "@/types"

// VmmInferenceSettingsProps exposes required-foundation readiness to the first-run workflow.
// VmmInferenceSettingsProps 向首次配置流程暴露必需基础能力就绪状态。
interface VmmInferenceSettingsProps {
  onRequiredRouteReady?: (ready: boolean) => void
  // Restarts the single managed VMM after a persisted rerank route change.
  // 在持久化重排路由变更后重启唯一的托管 VMM。
  onRestartVmm?: () => Promise<boolean>
  // Blocks inference-route mutations while the exclusive memory maintenance owner holds the data space.
  // 在记忆维护独占所有者持有数据空间时阻止推理路由变更。
  maintenanceActive?: boolean
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
    && candidate.route.usage_scope === "external_service"
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
    && candidateSupportsDisabledReasoning(candidate)
  ))
  return VMM_REQUIRED_LLM_PURPOSES.every((purposeId) => {
    const route = profile.llm_routes?.[purposeId]
    return Boolean(
      route
      && llmCandidates.some((candidate) => (
        inferenceRouteKey(candidate.route) === inferenceRouteKey(route)
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
  maintenanceActive = false,
  onRequiredRouteReady,
  onRestartVmm,
}: VmmInferenceSettingsProps = {}) {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [editingCandidate, setEditingCandidate] =
    React.useState<PublicAiInternalInferenceCandidate | null>(null)
  const [testingOperation, setTestingOperation] = React.useState<PublicAiOperation | null>(null)
  const [testSummaries, setTestSummaries] =
    React.useState<Partial<Record<PublicAiOperation, string>>>({})
  const [pendingMutationKey, setPendingMutationKey] = React.useState<string | null>(null)
  // rerankRestartConfirmationOpen keeps route writes blocked while the restart decision is visible.
  // rerankRestartConfirmationOpen 在显示重启决策时阻止路由写入。
  const [rerankRestartConfirmationOpen, setRerankRestartConfirmationOpen] =
    React.useState(false)
  // rerankRestartConfirmationResolver completes the deferred route change exactly once.
  // rerankRestartConfirmationResolver 只完成一次延迟的路由变更。
  const rerankRestartConfirmationResolver = React.useRef<((confirmed: boolean) => void) | null>(null)
  const mutationInFlightRef = React.useRef(false)

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

  // runSnapshotMutation serializes authoritative inference-profile replacements.
  // runSnapshotMutation 串行化权威推理配置档替换。
  const runSnapshotMutation = React.useCallback((
    key: string,
    mutation: () => Promise<PublicAiSettingsSnapshot>,
    // afterSuccess performs the controlled runtime action after the snapshot write succeeds.
    // afterSuccess 在快照写入成功后执行受控运行时动作。
    afterSuccess?: () => Promise<void>,
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
        if (afterSuccess) {
          await afterSuccess()
        }
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
  // VMM's issued access grant is fixed to external_service, so other scopes are not selectable.
  // VMM 签发的访问授权固定为 external_service，因此其他用途范围不可选择。
  const candidates = snapshot?.internalInferenceCandidates.filter((candidate) => (
    candidate.enabled
    && candidate.route.usage_scope === "external_service"
  )) ?? []
  // llmCandidates retain only exact contracts that can run with reasoning disabled.
  // llmCandidates 仅保留能够关闭思考执行的精确契约。
  const llmCandidates = candidates.filter((candidate) => (
    candidate.route.operation === "conversation_respond"
    && candidateSupportsDisabledReasoning(candidate)
  ))
  const diagnostics = snapshot?.inferenceRouteDiagnostics.filter((diagnostic) => (
    diagnostic.consumerProfileId === "vmm"
  )) ?? []
  // requiredRoutesReady gates first initialization on embedding plus all PreCheck/PostAction LLM routes.
  // requiredRoutesReady 使用向量路由和全部 PreCheck/PostAction LLM 路由共同门控首次初始化。
  const requiredRoutesReady = vmmRequiredInferenceReady(snapshot)
  // routesDisabled prevents settings writes while a route mutation, connection test, or data rebuild owns the workflow.
  // routesDisabled 在路由变更、连接测试或数据重建占用流程时阻止设置写入。
  const routesDisabled = maintenanceActive
    || pendingMutationKey !== null
    || testingOperation !== null
    || rerankRestartConfirmationOpen

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
            duration: result.durationMs,
          })
        : formatSkillText(t("settings.inference.testRerankSuccess"), {
            count: result.resultCount,
            duration: result.durationMs,
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
    // restartAfterSave keeps rerank persistence and the required managed-runtime restart in one guarded sequence.
    // restartAfterSave 将重排持久化与所需的托管运行时重启放在同一条受保护序列中。
    const restartAfterSave = operation === "rerank_documents" && onRestartVmm
      ? async () => {
          try {
            if (!await onRestartVmm()) {
              setError(t("settings.inference.rerankRestartFailed"))
            }
          } catch (reason) {
            setError(errorMessage(reason))
          }
        }
      : undefined
    return runSnapshotMutation(
      key,
      () => savePublicAiInferenceRoute({
        consumerProfileId: "vmm",
        purposeId,
        operation,
        route,
      }),
      restartAfterSave,
    )
  }, [onRestartVmm, runSnapshotMutation, t])

  // confirmRerankChange delays the route mutation until the user accepts the required VMM restart.
  // confirmRerankChange 在用户确认需要重启 VMM 前延迟路由变更。
  const confirmRerankChange = React.useCallback(() => {
    if (!onRestartVmm) {
      return Promise.resolve(true)
    }
    return new Promise<boolean>((resolve) => {
      rerankRestartConfirmationResolver.current = resolve
      setRerankRestartConfirmationOpen(true)
    })
  }, [onRestartVmm])

  // resolveRerankChangeConfirmation closes the dialog and resolves the pending route request exactly once.
  // resolveRerankChangeConfirmation 关闭对话框并且只解析一次等待中的路由请求。
  const resolveRerankChangeConfirmation = React.useCallback((confirmed: boolean) => {
    setRerankRestartConfirmationOpen(false)
    const resolve = rerankRestartConfirmationResolver.current
    rerankRestartConfirmationResolver.current = null
    resolve?.(confirmed)
  }, [])

  React.useEffect(() => () => {
    rerankRestartConfirmationResolver.current?.(false)
    rerankRestartConfirmationResolver.current = null
  }, [])

  return (
    <section className="rounded-xl border bg-card p-3">
      <div className="mb-3 flex items-start gap-2">
        <Database className="mt-0.5 shrink-0" size={17} />
        <div className="min-w-0">
          <strong>{t("settings.inference.title")} · {t("settings.inference.profileVmm")}</strong>
          <p className="mt-0.5 text-xs text-muted-foreground">{t("settings.inference.subtitle")}</p>
        </div>
      </div>
      <div className="mb-3 flex items-start gap-2 rounded-lg border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
        <Info className="mt-0.5 size-3.5 shrink-0" />
        <span>{t("settings.inference.vmmReasoningNotice")}</span>
      </div>
      {error ? <p className="settings-muted-line settings-error-text mb-2">{error}</p> : null}
      {maintenanceActive ? (
        <p className="mb-2 text-xs text-muted-foreground">
          {t("settings.inference.maintenanceLocked")}
        </p>
      ) : null}
      {!snapshot ? (
        <p className="settings-muted-line">{t("settings.models.loadingCatalog")}</p>
      ) : (
        <>
          <div className="grid gap-2 md:grid-cols-2">
            {purposes.map((purpose) => (
              <InferenceRouteField
                candidates={llmCandidates}
                disabled={routesDisabled}
                description={purpose.description}
                key={purpose.id}
                label={`${t("settings.inference.modelRegion")} · ${purpose.label}`}
                onConfigure={setEditingCandidate}
                onChange={(route) => saveRoute("conversation_respond", purpose.id, route)}
                route={profile?.llm_routes?.[purpose.id]}
                unavailableReason={diagnostics.find((diagnostic) => (
                  diagnostic.operation === "conversation_respond"
                  && diagnostic.purposeId === purpose.id
                ))?.reason}
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
              unavailableReason={diagnostics.find((diagnostic) => (
                diagnostic.operation === "embedding_create"
                && diagnostic.purposeId === undefined
              ))?.reason}
            />
            <InferenceRouteField
              candidates={candidates.filter((candidate) => (
                candidate.route.operation === "rerank_documents"
              ))}
              disabled={routesDisabled}
              label={t("settings.inference.rerank")}
              onConfigure={setEditingCandidate}
              onBeforeChange={confirmRerankChange}
              onChange={(route) => saveRoute("rerank_documents", undefined, route)}
              onTest={() => void testRoute("rerank_documents")}
              route={profile?.rerank}
              testPending={testingOperation === "rerank_documents"}
              testSummary={testSummaries.rerank_documents}
              unavailableReason={diagnostics.find((diagnostic) => (
                diagnostic.operation === "rerank_documents"
                && diagnostic.purposeId === undefined
              ))?.reason}
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
          <AlertDialog
            onOpenChange={(open) => {
              if (!open) {
                resolveRerankChangeConfirmation(false)
              }
            }}
            open={rerankRestartConfirmationOpen}
          >
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t("settings.inference.rerankRestartTitle")}</AlertDialogTitle>
                <AlertDialogDescription>
                  {t("settings.inference.rerankRestartDescription")}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel onClick={() => resolveRerankChangeConfirmation(false)}>
                  {t("settings.inference.rerankRestartCancel")}
                </AlertDialogCancel>
                <AlertDialogAction
                  onClick={(event) => {
                    event.preventDefault()
                    resolveRerankChangeConfirmation(true)
                  }}
                >
                  {t("settings.inference.rerankRestartConfirm")}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
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
  // fallbackAttemptKey prevents repeated writes when an unavailable persisted account cannot be replaced.
  // fallbackAttemptKey 防止不可用的已持久化账号替换失败时重复写入。
  const fallbackAttemptKey = React.useRef<string | null>(null)

  // requestRouteChange confirms before committing the local model draft and exact route.
  // requestRouteChange 在提交本地模型草稿和精确路由前执行确认。
  const requestRouteChange = React.useCallback(async (
    route: PublicAiInferenceRoute | null,
    nextModelKey: string | null,
  ) => {
    // nextRouteKey identifies the exact provider/account route before any confirmation or write.
    // nextRouteKey 在确认或写入前标识精确的提供商/账户路由。
    const nextRouteKey = route ? inferenceRouteKey(route) : ""
    if (persistedRouteKey === nextRouteKey) {
      setSelectedModelKey(nextModelKey)
      return
    }
    if (props.onBeforeChange && !await props.onBeforeChange(route)) {
      return
    }
    setSelectedModelKey(nextModelKey)
    // persisted is the asynchronous route-write outcome used to restore the previous draft on failure.
    // persisted 是异步路由写入结果，用于失败时恢复之前的草稿。
    const persisted = props.onChange(route)
    if (persisted instanceof Promise && !await persisted) {
      setSelectedModelKey(persistedModelKey)
    }
  }, [persistedModelKey, persistedRouteKey, props.onBeforeChange, props.onChange])

  React.useEffect(() => {
    setSelectedModelKey(persistedModelKey)
  }, [persistedRouteKey])

  React.useEffect(() => {
    if (
      !props.route
      || accountCandidates.length === 0
      || candidateByRouteKey.has(persistedRouteKey)
    ) {
      fallbackAttemptKey.current = null
      return
    }
    if (props.disabled) {
      return
    }
    // firstAvailableAccount is deterministic because backend candidates are sorted by account display name.
    // firstAvailableAccount 是确定性的，因为后端候选按账号显示名称排序。
    const firstAvailableAccount = accountCandidates[0]
    // attemptKey changes only when either the unavailable route or its replacement changes.
    // attemptKey 仅在不可用路由或替代路由发生变化时改变。
    const attemptKey = `${persistedRouteKey}->${inferenceRouteKey(firstAvailableAccount.route)}`
    if (fallbackAttemptKey.current === attemptKey) {
      return
    }
    fallbackAttemptKey.current = attemptKey
    void requestRouteChange(
      firstAvailableAccount.route,
      inferenceModelKey(firstAvailableAccount.route),
    )
  }, [
    accountCandidates,
    candidateByRouteKey,
    persistedRouteKey,
    props.disabled,
    props.route,
    requestRouteChange,
  ])

  return (
    <div className="min-w-0 rounded-lg border p-2">
      <div className="mb-1.5 flex min-w-0 items-center gap-1">
        <label className="block truncate text-xs font-medium">{props.label}</label>
        {props.description ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label={props.description}
                className="shrink-0 text-muted-foreground hover:text-foreground"
                type="button"
              >
                <Info className="size-3.5" aria-hidden="true" />
              </button>
            </TooltipTrigger>
            <TooltipContent className="max-w-72 text-xs" side="top">
              {props.description}
            </TooltipContent>
          </Tooltip>
        ) : null}
      </div>
      <div className="grid min-w-0 grid-cols-2 gap-1.5">
        <div className="min-w-0">
          <span className="mb-1 block text-[11px] text-muted-foreground">
            {t("settings.inference.modelRegion")}
          </span>
          <InferencePickerControl
            ariaLabel={`${props.label} · ${t("settings.inference.modelRegion")}`}
            disabled={props.disabled}
            emptyLabel={t("composer.modelPicker.noMatches")}
            filterPlaceholder={t("composer.modelPicker.filter")}
            icon={<Cpu className="size-4" />}
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
                void requestRouteChange(firstAvailableAccount.route, value)
              } else {
                setSelectedModelKey(value)
              }
            }}
            options={modelOptions}
            placeholder={t("settings.inference.selectModel")}
            selectedFallback={props.route?.model_id}
            value={selectedModelKey}
          />
        </div>
        <div className="min-w-0">
          <span className="mb-1 block text-[11px] text-muted-foreground">
            {t("settings.inference.accountRegion")}
          </span>
          <InferencePickerControl
            ariaLabel={`${props.label} · ${t("settings.inference.accountRegion")}`}
            disabled={props.disabled || selectedModelKey === null}
            emptyLabel={t("settings.inference.noAccountMatches")}
            filterPlaceholder={t("settings.inference.accountFilter")}
            icon={<UserRound className="size-4" />}
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
            value={selectedAccountKey}
          />
        </div>
      </div>
      <div className="mt-1.5 flex justify-end gap-1.5">
        <Button
          aria-label={t("settings.inference.configureDefaults")}
          className="size-9 shrink-0"
          disabled={props.disabled || !selectedCandidate}
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
          <SlidersHorizontal className="size-4" />
        </Button>
        {props.onTest ? (
          <Button
            aria-label={t("settings.inference.testConnection")}
            className="size-9 shrink-0"
            disabled={props.disabled || props.testPending || !selectedCandidate}
            onClick={props.onTest}
            size="icon"
            title={t("settings.inference.testConnection")}
            type="button"
            variant="outline"
          >
            {props.testPending ? (
              <LoaderCircle className="size-4 animate-spin" />
            ) : (
              <Play className="size-4" />
            )}
          </Button>
        ) : null}
      </div>
      {selectedCandidate ? (
        <div className="mt-1.5 space-y-0.5 break-words text-[11px] text-muted-foreground">
          <p>
            {selectedCandidate.serviceEntryDisplayName}
            {" · "}
            {t("settings.inference.credentialAvailable")}
          </p>
        </div>
      ) : null}
      {props.unavailableReason ? (
        <p className="mt-1.5 break-words text-xs text-destructive">
          {formatSkillText(t("settings.inference.unavailableReason"), {
            reason: props.unavailableReason,
          })}
        </p>
      ) : null}
      {props.testSummary ? (
        <p className="mt-1.5 break-words text-xs text-emerald-600 dark:text-emerald-400">
          {props.testSummary}
        </p>
      ) : null}
    </div>
  )
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
    <DropdownMenu
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen)
        if (!nextOpen) {
          setFilter("")
        }
      }}
      open={open}
    >
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={props.ariaLabel}
          className="h-9 w-full min-w-0 justify-start gap-2 overflow-hidden px-3 font-normal"
          disabled={props.disabled}
          type="button"
          variant="outline"
        >
          <span className="shrink-0 text-muted-foreground">{props.icon}</span>
          <span className="truncate">{triggerLabel}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="start"
        className="model-picker-content"
        sideOffset={8}
      >
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
            <div
              className={cn(
                "model-picker-group-row",
                props.value === null && "selected",
              )}
            >
              <Button
                className="model-picker-group-main"
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
            </div>
            {filteredOptions.length === 0 ? (
              <div className="model-picker-empty">{props.emptyLabel}</div>
            ) : (
              filteredOptions.map((option) => {
                // selected marks the exact model or account identity for this picker stage.
                // selected 标记当前选择阶段的精确模型或账号身份。
                const selected = option.key === props.value
                return (
                  <div
                    className={cn(
                      "model-picker-group-row",
                      selected && "selected",
                    )}
                    key={option.key}
                  >
                    <Button
                      className="model-picker-group-main"
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
                    </Button>
                    <span className="model-picker-row-id">{option.trailing}</span>
                  </div>
                )
              })
            )}
          </div>
        </ScrollArea>
      </DropdownMenuContent>
    </DropdownMenu>
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

// errorMessage converts one rejected value into a visible diagnostic.
// errorMessage 将一个被拒绝的值转换为可见诊断。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
