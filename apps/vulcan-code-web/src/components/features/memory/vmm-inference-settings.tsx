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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
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

// VmmInferenceSettings owns VMM inference route loading, mutation, and validation.
// VmmInferenceSettings 管理 VMM 推理路由的加载、变更与验证。
export function VmmInferenceSettings() {
  const { t } = useI18n()
  const [snapshot, setSnapshot] = React.useState<PublicAiSettingsSnapshot | null>(null)
  const [error, setError] = React.useState<string | null>(null)
  const [editingCandidate, setEditingCandidate] =
    React.useState<PublicAiInternalInferenceCandidate | null>(null)
  const [testingOperation, setTestingOperation] = React.useState<PublicAiOperation | null>(null)
  const [testSummaries, setTestSummaries] =
    React.useState<Partial<Record<PublicAiOperation, string>>>({})
  const [pendingMutationKey, setPendingMutationKey] = React.useState<string | null>(null)
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

  const profile = snapshot?.inferenceProfiles.vmm
  // VMM's issued access grant is fixed to external_service, so other scopes are not selectable.
  // VMM 签发的访问授权固定为 external_service，因此其他用途范围不可选择。
  const candidates = snapshot?.internalInferenceCandidates.filter((candidate) => (
    candidate.route.usage_scope === "external_service"
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
  const purposes: Array<{ id: string; label: string }> = [
    { id: "precheck_l1", label: t("settings.inference.purpose.precheckL1") },
    { id: "precheck_l2", label: t("settings.inference.purpose.precheckL2") },
    { id: "postaction_l1", label: t("settings.inference.purpose.postactionL1") },
    { id: "postaction_l2", label: t("settings.inference.purpose.postactionL2") },
    { id: "profile_instruction", label: t("settings.inference.purpose.profileInstruction") },
    { id: "reserve", label: t("settings.inference.purpose.reserve") },
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
    runSnapshotMutation(key, () => savePublicAiInferenceRoute({
      consumerProfileId: "vmm",
      purposeId,
      operation,
      route,
    }))
  }, [runSnapshotMutation])

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
      {!snapshot ? (
        <p className="settings-muted-line">{t("settings.models.loadingCatalog")}</p>
      ) : (
        <>
          <div className="grid gap-2 md:grid-cols-2">
            {purposes.map((purpose) => (
              <InferenceRouteField
                candidates={llmCandidates}
                disabled={pendingMutationKey !== null || testingOperation !== null}
                key={purpose.id}
                label={`${t("settings.inference.llm")} · ${purpose.label}`}
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
              disabled={pendingMutationKey !== null || testingOperation !== null}
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
              disabled={pendingMutationKey !== null || testingOperation !== null}
              label={t("settings.inference.rerank")}
              onConfigure={setEditingCandidate}
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
  onChange: (route: PublicAiInferenceRoute | null) => void
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
    props.onChange(firstAvailableAccount.route)
  }, [
    accountCandidates,
    candidateByRouteKey,
    persistedRouteKey,
    props.disabled,
    props.onChange,
    props.route,
  ])

  return (
    <div className="min-w-0 rounded-lg border p-2">
      <label className="mb-1.5 block truncate text-xs font-medium">{props.label}</label>
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
              setSelectedModelKey(value)
              if (value === null) {
                props.onChange(null)
                return
              }
              if (props.route && value === persistedModelKey) {
                return
              }
              // firstAvailableAccount immediately applies the first account for a newly selected model.
              // firstAvailableAccount 会在选择新模型时立即应用第一个账号。
              const firstAvailableAccount = props.candidates.find((candidate) => (
                inferenceModelKey(candidate.route) === value
              ))
              if (firstAvailableAccount) {
                props.onChange(firstAvailableAccount.route)
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
              props.onChange(value ? candidateByRouteKey.get(value)?.route ?? null : null)
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
          <p>
            {formatSkillText(t("settings.inference.catalogGeneration"), {
              generation: selectedCandidate.catalogGeneration,
            })}
            {" · "}
            {formatSkillText(t("settings.inference.capabilityRevision"), {
              revision: selectedCandidate.capabilities.evidence_revision,
            })}
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
