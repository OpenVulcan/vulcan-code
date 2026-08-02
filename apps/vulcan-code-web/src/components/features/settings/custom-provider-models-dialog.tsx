import * as React from "react"
import {
  ArrowLeft,
  CheckCheck,
  CircleAlert,
  Cloud,
  FilePlus2,
  LoaderCircle,
  Plus,
  RefreshCw,
  Search,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Users,
  X,
} from "lucide-react"

import { CustomModelTemplateCard } from "@/components/features/settings/custom-model-template-card"
import { CustomProviderChangeSummaryDialog } from "@/components/features/settings/custom-provider-change-summary-dialog"
import {
  advancedModelTextDraftFromDefinition,
  applyAdvancedModelTextDrafts,
  type AdvancedModelTextDraft,
} from "@/components/features/settings/custom-provider-model-drafts"
import { CustomProviderModelDefaultsCard } from "@/components/features/settings/custom-provider-model-defaults-card"
import {
  accountActuallyDiscoveredModel,
  accountExposesModel,
  buildCustomProviderModelRows,
  shouldBlockManualModelSubmission,
  shouldReplaceManualModelDrafts,
  type CustomProviderModelAccountState,
  type CustomProviderModelRow,
} from "@/components/features/settings/custom-provider-models-state"
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import {
  getPublicAiCustomModelTemplateSuggestions,
  getPublicAiCustomModelTemplateSuggestionsBatch,
  loadPublicAiSettings,
  savePublicAiCustomModelConnectionAvailability,
  savePublicAiCustomModelTemplateDecisionsBatch,
  savePublicAiCustomModels,
  savePublicAiModelCapabilityOverrides,
  savePublicAiModelEnabled,
  savePublicAiProviderModelsEnabled,
  syncPublicAiConnection,
  validatePublicAiCustomModel,
} from "@/service/settings-actions-service"
import type {
  PublicAiCommonModelTemplateSuggestion,
  PublicAiCustomModelDefinition,
  PublicAiCustomModelRequestProjection,
  PublicAiCustomProviderModelDefaults,
  PublicAiCustomModelTemplateDecision,
  PublicAiCustomModelTemplateDecisionBatchItem,
  PublicAiCustomModelTemplateSuggestionsRecord,
  PublicAiModelCapabilityOverrides,
  PublicAiProviderDescriptor,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
} from "@/types"

// CustomProviderModelsDialogProps identifies the provider and immutable runtime snapshot being edited.
// CustomProviderModelsDialogProps 标识被编辑供应商及其不可变运行时快照。
interface CustomProviderModelsDialogProps {
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  provider: PublicAiProviderDescriptor | null
  snapshot: PublicAiSettingsSnapshot | null
}

// CustomModelFilter is the user-facing enabled-state filter.
// CustomModelFilter 是面向用户的启用状态筛选器。
type CustomModelFilter = "all" | "enabled" | "disabled"

// ReasoningSupport is the exact serialized provider-neutral fidelity option shown by the editor.
// ReasoningSupport 是编辑器展示的精确供应商无关保真度选项。
type ReasoningSupport = "unknown" | "native" | "equivalent" | "lossy" | "unsupported"

// EMPTY_PROJECTION is the safe request projection used until the user declares exact mappings.
// EMPTY_PROJECTION 是用户声明精确映射前使用的安全请求投影。
const EMPTY_PROJECTION: PublicAiCustomModelRequestProjection = {
  reasoning: {},
  model_parameter_paths: {},
  defaults: {},
  overrides: {},
  delete: [],
}

// FALLBACK_CUSTOM_PROVIDER_MODEL_DEFAULTS is the documented safe baseline before any user override.
// FALLBACK_CUSTOM_PROVIDER_MODEL_DEFAULTS 是用户尚未覆盖时使用的文档化安全基线。
const FALLBACK_CUSTOM_PROVIDER_MODEL_DEFAULTS: PublicAiCustomProviderModelDefaults = {
  context_limit: 200_000,
  output_limit: 32_768,
  input_resources: [],
  output_resources: [],
}

// RESOURCE_KINDS is the closed non-text modality set accepted by the public contract.
// RESOURCE_KINDS 是公共契约接受的封闭非文本模态集合。
const RESOURCE_KINDS: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// RESOURCE_LABEL_KEYS maps resource kinds to localized labels without dynamic translation keys.
// RESOURCE_LABEL_KEYS 将资源类型映射到本地化标签且不使用动态翻译键。
const RESOURCE_LABEL_KEYS = {
  image: "settings.models.modality.image",
  audio: "settings.models.modality.audio",
  video: "settings.models.modality.video",
  file: "settings.models.modality.file",
} as const

// DISCOVERY_STATE_LABEL_KEYS maps every closed backend status to one localized label.
// DISCOVERY_STATE_LABEL_KEYS 将每个封闭后端状态映射为一个本地化标签。
const DISCOVERY_STATE_LABEL_KEYS = {
  not_checked: "settings.customModels.discovery.notChecked",
  ready: "settings.customModels.discovery.ready",
  empty: "settings.customModels.discovery.empty",
  unsupported: "settings.customModels.discovery.unsupported",
  authentication_failed: "settings.customModels.discovery.authenticationFailed",
  rate_limited: "settings.customModels.discovery.rateLimited",
  temporarily_unavailable: "settings.customModels.discovery.temporarilyUnavailable",
  invalid_response: "settings.customModels.discovery.invalidResponse",
  stale: "settings.customModels.discovery.stale",
  failed: "settings.customModels.discovery.failed",
} as const

// SUPPORT_LABEL_KEYS maps serialized support fidelity values to localized labels.
// SUPPORT_LABEL_KEYS 将序列化支持保真度映射为本地化标签。
const SUPPORT_LABEL_KEYS = {
  unknown: "settings.customModels.supportUnknown",
  native: "settings.customModels.supportNative",
  equivalent: "settings.customModels.supportEquivalent",
  lossy: "settings.customModels.supportLossy",
  unsupported: "settings.customModels.supportUnsupported",
} as const

// REASONING_CONTROL_LABEL_KEYS maps exact request behaviors to localized labels.
// REASONING_CONTROL_LABEL_KEYS 将精确请求行为映射为本地化标签。
const REASONING_CONTROL_LABEL_KEYS = {
  unknown: "settings.customModels.controlUnknown",
  fixed_off: "settings.customModels.controlFixedOff",
  fixed_on: "settings.customModels.controlFixedOn",
  toggle: "settings.customModels.controlToggle",
  effort_selection: "settings.customModels.controlEffortSelection",
  provider_adaptive: "settings.customModels.controlProviderAdaptive",
} as const

// REASONING_EFFORTS is the closed common effort order accepted by the public contract.
// REASONING_EFFORTS 是公共契约接受的封闭通用推理档位顺序。
const REASONING_EFFORTS = ["none", "adaptive", "minimal", "low", "medium", "high", "x_high", "max", "ultra"] as const

// REASONING_EFFORT_LABEL_KEYS maps canonical effort values to localized labels.
// REASONING_EFFORT_LABEL_KEYS 将规范思考档位映射为本地化标签。
const REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  adaptive: "settings.customModels.effortAdaptive",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
  medium: "settings.customModels.effortMedium",
  high: "settings.customModels.effortHigh",
  x_high: "settings.customModels.effortExtraHigh",
  max: "settings.customModels.effortMax",
  ultra: "settings.customModels.effortUltra",
} as const

// CustomProviderModelsDialog manages one merged model union and credential-scoped evidence.
// CustomProviderModelsDialog 管理一个合并模型并集与凭据作用域证据。
export function CustomProviderModelsDialog(props: CustomProviderModelsDialogProps) {
  const { t } = useI18n()
  const [manualModels, setManualModels] = React.useState<PublicAiCustomModelDefinition[]>([])
  const [manualModelsDirty, setManualModelsDirty] = React.useState(false)
  const [selectedModelId, setSelectedModelId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [filter, setFilter] = React.useState<CustomModelFilter>("all")
  const [advancedTextDrafts, setAdvancedTextDrafts] = React.useState<Record<string, AdvancedModelTextDraft>>({})
  const [dirtyManualModelIds, setDirtyManualModelIds] = React.useState<Set<string>>(new Set())
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [removeConfirmationOpen, setRemoveConfirmationOpen] = React.useState(false)
  const [manualDialogOpen, setManualDialogOpen] = React.useState(false)
  const [mobileDetailOpen, setMobileDetailOpen] = React.useState(false)
  const [selectedConnectionId, setSelectedConnectionId] = React.useState<string | null>(null)
  const [discardConfirmationOpen, setDiscardConfirmationOpen] = React.useState(false)
  const [advancedSaveConfirmationOpen, setAdvancedSaveConfirmationOpen] = React.useState(false)
  const [templateReviewOpen, setTemplateReviewOpen] = React.useState(false)

  React.useEffect(() => {
    if (!props.open || !props.provider || !props.snapshot || manualModelsDirty) {
      return
    }
    setManualModels(
      Object.values(props.snapshot.customModels?.[props.provider.id] ?? {}).map(cloneCustomModel),
    )
    setAdvancedTextDrafts({})
    setDirtyManualModelIds(new Set())
  }, [manualModelsDirty, props.open, props.provider, props.snapshot])

  const workingSnapshot = React.useMemo(() => {
    if (!props.snapshot || !props.provider) {
      return null
    }
    return {
      ...props.snapshot,
      customModels: {
        ...(props.snapshot.customModels ?? {}),
        [props.provider.id]: Object.fromEntries(manualModels.map((model) => [model.id, model])),
      },
    }
  }, [manualModels, props.provider, props.snapshot])

  const rows = React.useMemo(() => {
    if (!workingSnapshot || !props.provider) {
      return []
    }
    return buildCustomProviderModelRows(workingSnapshot, props.provider.id)
  }, [props.provider, workingSnapshot])

  React.useEffect(() => {
    if (rows.length === 0) {
      setSelectedModelId(null)
      return
    }
    if (!selectedModelId || !rows.some((row) => row.id === selectedModelId)) {
      setSelectedModelId(rows[0].id)
    }
  }, [rows, selectedModelId])

  const selectedRow = rows.find((row) => row.id === selectedModelId) ?? null
  React.useEffect(() => {
    if (!selectedRow || selectedRow.accounts.length === 0) {
      setSelectedConnectionId(null)
      return
    }
    if (selectedConnectionId && selectedRow.accounts.some((account) => account.connection.id === selectedConnectionId)) {
      return
    }
    const preferredAccount = selectedRow.accounts.find(accountExposesModel)
      ?? selectedRow.accounts.find(accountActuallyDiscoveredModel)
      ?? selectedRow.accounts[0]
    setSelectedConnectionId(preferredAccount.connection.id)
  }, [selectedConnectionId, selectedRow])
  const selectedAccount = selectedRow?.accounts.find(
    (account) => account.connection.id === selectedConnectionId,
  ) ?? selectedRow?.accounts[0]
  const visibleRows = React.useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase()
    return rows.filter((row) => {
      if (filter === "enabled" && !row.enabled) return false
      if (filter === "disabled" && row.enabled) return false
      return !normalizedQuery
        || row.id.toLocaleLowerCase().includes(normalizedQuery)
        || row.displayName.toLocaleLowerCase().includes(normalizedQuery)
    })
  }, [filter, query, rows])

  const providerDefaults = props.provider && props.snapshot
    ? props.snapshot.customProviderModelDefaults?.[props.provider.id]
      ?? FALLBACK_CUSTOM_PROVIDER_MODEL_DEFAULTS
    : null

  const selectedAdvancedDraft = selectedRow?.manualDefinition
    ? advancedTextDrafts[selectedRow.id]
      ?? advancedModelTextDraftFromDefinition(selectedRow.manualDefinition)
    : {
      projectionText: JSON.stringify(EMPTY_PROJECTION, null, 2),
      parameterText: "[]",
    }

  // acceptSnapshot publishes a service result without discarding unrelated local advanced drafts.
  // acceptSnapshot 发布服务结果且不会丢弃无关的本地高级草稿。
  const acceptSnapshot = React.useCallback((
    snapshot: PublicAiSettingsSnapshot,
    manualCommitSucceeded = false,
  ) => {
    props.onSaved(snapshot)
    if (
      props.provider
      && shouldReplaceManualModelDrafts(manualModelsDirty, manualCommitSucceeded)
    ) {
      setManualModels(Object.values(snapshot.customModels?.[props.provider.id] ?? {}).map(cloneCustomModel))
      setAdvancedTextDrafts({})
      setDirtyManualModelIds(new Set())
      setManualModelsDirty(false)
    }
    if (manualCommitSucceeded) {
      setAdvancedSaveConfirmationOpen(false)
    }
  }, [manualModelsDirty, props])

  // persistManualModels validates advanced JSON and writes only user-owned model definitions.
  // persistManualModels 校验高级 JSON 并仅写入用户自有模型定义。
  const persistManualModels = React.useCallback(async (
    replacement: PublicAiCustomModelDefinition[] = manualModels,
  ) => {
    if (!props.provider) return null
    setSaving(true)
    setError(null)
    try {
      const committed = applyAdvancedModelTextDrafts(replacement, advancedTextDrafts, {
        projectionInvalid: t("settings.customModels.projectionInvalid"),
        parametersInvalid: t("settings.customModels.parametersInvalid"),
      })
      const snapshot = await savePublicAiCustomModels(props.provider.id, committed)
      acceptSnapshot(snapshot, true)
      return snapshot
    } catch (reason) {
      setError(errorMessage(reason))
      return null
    } finally {
      setSaving(false)
    }
  }, [acceptSnapshot, advancedTextDrafts, manualModels, props.provider, t])

  // updateManualDefinition changes one local advanced model declaration before explicit save.
  // updateManualDefinition 在显式保存前修改一个本地高级模型声明。
  const updateManualDefinition = React.useCallback((
    modelId: string,
    update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition,
  ) => {
    setManualModels((current) => current.map((model) => model.id === modelId ? update(model) : model))
    setDirtyManualModelIds((current) => new Set(current).add(modelId))
    setManualModelsDirty(true)
  }, [])

  // updateAdvancedTextDraft records raw JSON text without losing sibling-model edits.
  // updateAdvancedTextDraft 记录原始 JSON 文本且不会丢失其他模型的编辑。
  const updateAdvancedTextDraft = React.useCallback((
    model: PublicAiCustomModelDefinition,
    field: keyof AdvancedModelTextDraft,
    value: string,
  ) => {
    setAdvancedTextDrafts((current) => {
      const existing = current[model.id] ?? advancedModelTextDraftFromDefinition(model)
      return { ...current, [model.id]: { ...existing, [field]: value } }
    })
    setDirtyManualModelIds((current) => new Set(current).add(model.id))
    setManualModelsDirty(true)
  }, [])

  // synchronizeAll refreshes every exact credential and preserves successful sibling results.
  // synchronizeAll 刷新每个精确凭据，并保留同级账号中已经成功的结果。
  const synchronizeAll = React.useCallback(async () => {
    if (!props.provider || !props.snapshot) return
    setSaving(true)
    setError(null)
    // failures retains every account-scoped failure instead of stopping after the first account.
    // failures 保留每个账号作用域失败，而不是在首个账号失败后停止。
    const failures: string[] = []
    // connections is the immutable provider-scoped work queue captured before synchronization starts.
    // connections 是同步开始前捕获的不可变供应商作用域工作队列。
    const connections = props.snapshot.connections.filter((item) => item.provider_id === props.provider?.id)
    for (const connection of connections) {
      try {
        props.onSaved(await syncPublicAiConnection(connection.id))
      } catch (reason) {
        failures.push(`${connection.display_name}: ${errorMessage(reason)}`)
      }
    }
    try {
      props.onSaved(await loadPublicAiSettings())
    } catch (reason) {
      failures.push(errorMessage(reason))
    }
    if (failures.length > 0) {
      setError(failures.join("\n"))
    }
    setSaving(false)
  }, [props])

  // synchronizeConnection refreshes one exact credential without discarding local advanced edits.
  // synchronizeConnection 刷新一个精确凭据且不会丢弃本地高级编辑。
  const synchronizeConnection = React.useCallback(async (connectionId: string) => {
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await syncPublicAiConnection(connectionId))
    } catch (reason) {
      setError(errorMessage(reason))
      try {
        props.onSaved(await loadPublicAiSettings())
      } catch {
        // The original synchronization failure remains the actionable user-facing error.
        // 原始同步失败仍是面向用户的可操作错误。
      }
    } finally {
      setSaving(false)
    }
  }, [props])

  // requestDialogOpenChange protects local advanced edits from an accidental close.
  // requestDialogOpenChange 防止本地高级编辑因误关闭而丢失。
  const requestDialogOpenChange = React.useCallback((open: boolean) => {
    if (!open && manualModelsDirty) {
      setDiscardConfirmationOpen(true)
      return
    }
    props.onOpenChange(open)
  }, [manualModelsDirty, props])

  // setModelEnabled persists one provider-model access decision without changing accounts.
  // setModelEnabled 持久化一个供应商模型访问决定且不改变账号。
  const setModelEnabled = React.useCallback(async (row: CustomProviderModelRow, enabled: boolean) => {
    if (!props.provider) return
    setSaving(true)
    setError(null)
    try {
      acceptSnapshot(await savePublicAiModelEnabled({
        providerId: props.provider.id,
        modelId: row.id,
        enabled,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [acceptSnapshot, props.provider])

  // setAllEnabled applies one provider-scoped state to the synchronized model union.
  // setAllEnabled 将一个供应商作用域状态应用到同步模型并集。
  const setAllEnabled = React.useCallback(async (enabled: boolean) => {
    if (!props.provider) return
    setSaving(true)
    setError(null)
    try {
      acceptSnapshot(await savePublicAiProviderModelsEnabled({ providerId: props.provider.id, enabled }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [acceptSnapshot, props.provider])

  // removeManualDefinition removes only user-owned state; a remotely discovered model remains visible.
  // removeManualDefinition 仅移除用户自有状态；远程发现模型仍保持可见。
  const removeManualDefinition = React.useCallback(async () => {
    if (!selectedRow?.manualDefinition) return
    const replacement = manualModels.filter((model) => model.id !== selectedRow.id)
    const snapshot = await persistManualModels(replacement)
    if (snapshot) {
      setRemoveConfirmationOpen(false)
    }
  }, [manualModels, persistManualModels, selectedRow])

  return (
    <Dialog onOpenChange={requestDialogOpenChange} open={props.open}>
      <DialogContent className="flex h-[min(860px,94vh)] w-[min(1180px,96vw)] max-w-[min(1180px,96vw)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(1180px,96vw)]">
        <DialogHeader className="border-b px-5 py-4">
          <DialogTitle>{t("settings.customModels.title").replace("{provider}", props.provider?.display_name ?? "")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.description")}</DialogDescription>
        </DialogHeader>
        {props.provider && providerDefaults ? (
          <div className="grid gap-3 border-b p-4 lg:grid-cols-[minmax(0,1fr)_minmax(360px,0.8fr)]">
            <DiscoverySummary
              onRefresh={() => void synchronizeAll()}
              onRefreshConnection={(connectionId) => void synchronizeConnection(connectionId)}
              onReviewTemplates={() => setTemplateReviewOpen(true)}
              providerId={props.provider.id}
              snapshot={props.snapshot}
            />
            <CustomProviderModelDefaultsCard
              defaults={providerDefaults}
              onSaved={acceptSnapshot}
              providerId={props.provider.id}
            />
          </div>
        ) : null}
        <div className="grid min-h-0 flex-1 md:grid-cols-[320px_minmax(0,1fr)]">
          <section className={`${mobileDetailOpen ? "max-md:hidden" : ""} flex min-h-0 flex-col border-r`}>
            <div className="space-y-3 border-b p-3">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input className="pl-9" onChange={(event) => setQuery(event.target.value)} placeholder={t("settings.customModels.search")} value={query} />
              </div>
              <div className="flex gap-2">
                <Select onValueChange={(value) => setFilter(value as CustomModelFilter)} value={filter}>
                  <SelectTrigger className="min-w-0 flex-1"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">{t("settings.customModels.filterAll")}</SelectItem>
                    <SelectItem value="enabled">{t("settings.customModels.filterEnabled")}</SelectItem>
                    <SelectItem value="disabled">{t("settings.customModels.filterDisabled")}</SelectItem>
                  </SelectContent>
                </Select>
                <Tooltip delayDuration={500}>
                  <TooltipTrigger asChild>
                    <Button aria-label={t("settings.customModels.addManual")} onClick={() => setManualDialogOpen(true)} size="icon" type="button">
                      <Plus className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{t("settings.customModels.addManual")}</TooltipContent>
                </Tooltip>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Button disabled={saving || rows.length === 0} onClick={() => void setAllEnabled(true)} size="sm" type="button" variant="outline"><CheckCheck className="mr-2 size-4" />{t("settings.customModels.enableAll")}</Button>
                <Button disabled={saving || rows.length === 0} onClick={() => void setAllEnabled(false)} size="sm" type="button" variant="outline"><X className="mr-2 size-4" />{t("settings.customModels.disableAll")}</Button>
              </div>
            </div>
            <ScrollArea className="min-h-0 flex-1">
              <div className="space-y-1 p-2">
                {visibleRows.map((row) => (
                  <div
                    className={`flex w-full items-start gap-3 rounded-lg pr-3 ${selectedModelId === row.id ? "bg-accent text-accent-foreground" : "hover:bg-muted"}`}
                    key={row.id}
                    style={{ containIntrinsicSize: "64px", contentVisibility: "auto" }}
                  >
                    <button
                      className="min-w-0 flex-1 px-3 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                      onClick={() => { setSelectedModelId(row.id); setMobileDetailOpen(true) }}
                      type="button"
                    >
                      <span className="block truncate text-sm font-medium">{row.displayName}</span>
                      <span className="block truncate font-mono text-xs text-muted-foreground">{row.id}</span>
                      <span className="mt-1 flex flex-wrap gap-1">
                        {row.accounts.some(accountActuallyDiscoveredModel) ? <Badge variant="outline"><Cloud className="mr-1 size-3" />{t("settings.customModels.sourceApi")}</Badge> : null}
                        {row.manualDefinition ? <Badge variant="outline"><FilePlus2 className="mr-1 size-3" />{t("settings.customModels.sourceManual")}</Badge> : null}
                        <Badge variant="secondary"><Users className="mr-1 size-3" />{row.accounts.filter(accountExposesModel).length}</Badge>
                      </span>
                    </button>
                    <Switch
                      aria-label={`${row.displayName} · ${t("settings.customModels.enabled")}`}
                      checked={row.enabled}
                      className="mt-3"
                      onCheckedChange={(enabled) => void setModelEnabled(row, enabled)}
                    />
                  </div>
                ))}
                {visibleRows.length === 0 ? <p className="px-3 py-10 text-center text-sm text-muted-foreground">{t("settings.customModels.empty")}</p> : null}
              </div>
            </ScrollArea>
          </section>
          <section className={`${mobileDetailOpen ? "" : "max-md:hidden"} min-h-0`}>
            {selectedRow && props.provider && props.snapshot ? (
              <ScrollArea className="h-full">
                <div className="space-y-4 p-5">
                  <Button className="md:hidden" onClick={() => setMobileDetailOpen(false)} size="sm" type="button" variant="ghost"><ArrowLeft className="mr-2 size-4" />{t("settings.customModels.backToList")}</Button>
                  <ModelDetailHeader onEnabledChange={(enabled) => void setModelEnabled(selectedRow, enabled)} row={selectedRow} />
                  <CustomModelTemplateCard decision={selectedRow.templateDecision} modelId={selectedRow.id} onSaved={acceptSnapshot} providerId={props.provider.id} />
                  <Tabs defaultValue="overview">
                    <TabsList className="grid w-full grid-cols-4">
                      <TabsTrigger value="overview">{t("settings.customModels.tabOverview")}</TabsTrigger>
                      <TabsTrigger value="accounts">{t("settings.customModels.tabAccounts")}</TabsTrigger>
                      <TabsTrigger value="capabilities">{t("settings.customModels.tabCapabilities")}</TabsTrigger>
                      <TabsTrigger value="advanced">{t("settings.customModels.tabAdvanced")}</TabsTrigger>
                    </TabsList>
                    <TabsContent className="space-y-4" value="overview">
                      <CapabilityAccountSelector
                        connectionId={selectedAccount?.connection.id ?? ""}
                        onConnectionChange={setSelectedConnectionId}
                        row={selectedRow}
                      />
                      <ResolvedCapabilitySummary account={selectedAccount} defaults={providerDefaults} row={selectedRow} />
                    </TabsContent>
                    <TabsContent className="space-y-3" value="accounts">
                      <AccountAvailabilityEditor
                        onAccountSelected={setSelectedConnectionId}
                        onSaved={acceptSnapshot}
                        providerId={props.provider.id}
                        row={selectedRow}
                        selectedConnectionId={selectedAccount?.connection.id ?? null}
                      />
                    </TabsContent>
                    <TabsContent value="capabilities">
                      <CapabilityOverridesEditor onSaved={acceptSnapshot} providerId={props.provider.id} row={selectedRow} />
                    </TabsContent>
                    <TabsContent className="space-y-4" value="advanced">
                      <AdvancedModelEditor
                        manualDefinition={selectedRow.manualDefinition}
                        onCreateOverlay={async () => {
                          const model = newManualModel(selectedRow.id, selectedRow.displayName)
                          await persistManualModels([...manualModels, model])
                        }}
                        onRemove={() => setRemoveConfirmationOpen(true)}
                        onSave={() => setAdvancedSaveConfirmationOpen(true)}
                        onUpdate={(update) => updateManualDefinition(selectedRow.id, update)}
                        parameterText={selectedAdvancedDraft.parameterText}
                        projectionText={selectedAdvancedDraft.projectionText}
                        setParameterText={(value) => {
                          if (selectedRow.manualDefinition) {
                            updateAdvancedTextDraft(selectedRow.manualDefinition, "parameterText", value)
                          }
                        }}
                        setProjectionText={(value) => {
                          if (selectedRow.manualDefinition) {
                            updateAdvancedTextDraft(selectedRow.manualDefinition, "projectionText", value)
                          }
                        }}
                        saving={saving}
                      />
                    </TabsContent>
                  </Tabs>
                </div>
              </ScrollArea>
            ) : (
              <div className="grid h-full place-items-center p-8 text-center text-sm text-muted-foreground">{t("settings.customModels.selectHint")}</div>
            )}
          </section>
        </div>
        {error ? <p className="whitespace-pre-wrap border-t bg-destructive/5 px-5 py-3 text-sm text-destructive">{error}</p> : null}
        <DialogFooter className="border-t px-5 py-3">
          {manualModelsDirty ? <span className="mr-auto text-sm text-amber-600">{t("settings.customModels.unsavedAdvanced")}</span> : null}
          <Button onClick={() => requestDialogOpenChange(false)} type="button" variant="outline">{t("memory.common.close")}</Button>
          {manualModelsDirty ? <Button disabled={saving} onClick={() => setAdvancedSaveConfirmationOpen(true)} type="button">{saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t("settings.customModels.save")}</Button> : null}
        </DialogFooter>
      </DialogContent>
      <ManualModelDialog
        defaults={providerDefaults ?? FALLBACK_CUSTOM_PROVIDER_MODEL_DEFAULTS}
        existingIds={new Set(rows.map((row) => row.id))}
        onOpenChange={setManualDialogOpen}
        onSaved={async (model, decision) => {
          if (!props.provider) return
          // committed combines the new identity with all still-valid advanced drafts.
          // committed 将新模型身份与全部仍有效的高级草稿合并。
          const committed = applyAdvancedModelTextDrafts([...manualModels, model], advancedTextDrafts, {
            projectionInvalid: t("settings.customModels.projectionInvalid"),
            parametersInvalid: t("settings.customModels.parametersInvalid"),
          })
          // templateDecisions keeps model creation and its reviewed template choice in one transaction.
          // templateDecisions 让模型创建与其已审查模板选择处于同一事务。
          const templateDecisions: PublicAiCustomModelTemplateDecisionBatchItem[] = decision
            ? [{ modelId: model.id, decision }]
            : []
          // snapshot is published only after the complete atomic save succeeds.
          // snapshot 仅在完整原子保存成功后发布。
          const snapshot = await savePublicAiCustomModels(
            props.provider.id,
            committed,
            templateDecisions,
          )
          acceptSnapshot(snapshot, true)
          setSelectedModelId(model.id)
          setMobileDetailOpen(true)
          setManualDialogOpen(false)
        }}
        open={manualDialogOpen}
      />
      {props.provider ? (
        <TemplateReviewDialog
          onOpenChange={setTemplateReviewOpen}
          onSaved={acceptSnapshot}
          open={templateReviewOpen}
          providerId={props.provider.id}
          rows={rows}
        />
      ) : null}
      <AlertDialog onOpenChange={setRemoveConfirmationOpen} open={removeConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.remove")}</AlertDialogTitle>
            <AlertDialogDescription>{t("settings.customModels.removeConfirm").replace("{name}", selectedRow?.displayName ?? "")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void removeManualDefinition()} variant="destructive">{t("settings.customModels.remove")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog onOpenChange={setDiscardConfirmationOpen} open={discardConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.discardTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("settings.customModels.discardDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setManualModelsDirty(false)
                setAdvancedTextDrafts({})
                setDirtyManualModelIds(new Set())
                setDiscardConfirmationOpen(false)
                props.onOpenChange(false)
              }}
              variant="destructive"
            >
              {t("settings.customModels.discard")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <CustomProviderChangeSummaryDialog
        description={t("settings.customModels.advancedChangeSummaryDescription")}
        error={error}
        items={[
          {
            label: t("settings.customModels.summaryAdvancedDefinitions"),
            value: [...dirtyManualModelIds].sort().join(", ")
              || t("settings.customModels.summaryCurrentModel"),
          },
        ]}
        onConfirm={() => void persistManualModels()}
        onOpenChange={(open) => {
          if (!saving) setAdvancedSaveConfirmationOpen(open)
        }}
        open={advancedSaveConfirmationOpen}
        saving={saving}
      />
    </Dialog>
  )
}

// TemplateReviewDialog groups exact, similar, and missing offline capability suggestions.
// TemplateReviewDialog 对精确、相似和无命中的离线能力建议进行分组。
function TemplateReviewDialog(props: {
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  providerId: string
  rows: CustomProviderModelRow[]
}) {
  const { t } = useI18n()
  const [records, setRecords] = React.useState<PublicAiCustomModelTemplateSuggestionsRecord[]>([])
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  // modelIdsSignature changes only when the provider model union changes, not after a decision save.
  // modelIdsSignature 仅在供应商模型并集变化时改变，不会因保存决策而改变。
  const modelIdsSignature = React.useMemo(
    () => props.rows.map((row) => row.id).join("\u0000"),
    [props.rows],
  )

  React.useEffect(() => {
    if (!props.open) {
      return
    }
    const modelIds = modelIdsSignature ? modelIdsSignature.split("\u0000") : []
    if (modelIds.length === 0) {
      setRecords([])
      setError(null)
      return
    }
    let active = true
    setLoading(true)
    setError(null)
    void getPublicAiCustomModelTemplateSuggestionsBatch(modelIds)
      .then((nextRecords) => {
        if (active) {
          setRecords(nextRecords)
        }
      })
      .catch((reason) => {
        if (active) {
          setError(errorMessage(reason))
        }
      })
      .finally(() => {
        if (active) {
          setLoading(false)
        }
      })
    return () => {
      active = false
    }
  }, [modelIdsSignature, props.open])

  // rowById joins immutable suggestion results with current persisted decisions.
  // rowById 将不可变建议结果与当前持久化决策连接起来。
  const rowById = React.useMemo(
    () => new Map(props.rows.map((row) => [row.id, row])),
    [props.rows],
  )
  // undecidedRecords excludes decisions already explicitly accepted or rejected by the user.
  // undecidedRecords 排除用户已经明确接受或拒绝的决策。
  const undecidedRecords = records.filter((record) => !rowById.get(record.modelId)?.templateDecision)
  const exactRecords = undecidedRecords.filter((record) => record.suggestions[0]?.match_kind !== "similar" && record.suggestions.length > 0)
  const similarRecords = undecidedRecords.filter((record) => record.suggestions[0]?.match_kind === "similar")
  const missingRecords = undecidedRecords.filter((record) => record.suggestions.length === 0)

  // saveDecisions sends reviewed decisions as one atomic provider-scoped update.
  // saveDecisions 将已审查决策作为一次供应商作用域原子更新发送。
  const saveDecisions = React.useCallback(async (
    decisions: PublicAiCustomModelTemplateDecisionBatchItem[],
  ) => {
    if (decisions.length === 0) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiCustomModelTemplateDecisionsBatch({
        providerId: props.providerId,
        decisions,
      }))
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [props])

  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="flex h-[min(760px,92vh)] w-[min(840px,94vw)] max-w-none flex-col gap-0 overflow-hidden p-0">
        <DialogHeader className="border-b px-5 py-4">
          <DialogTitle>{t("settings.customModels.reviewTemplatesTitle")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.reviewTemplatesDescription")}</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-3 gap-3 border-b p-4">
          <TemplateReviewCount label={t("settings.customModels.reviewExact")} value={exactRecords.length} />
          <TemplateReviewCount label={t("settings.customModels.reviewSimilar")} value={similarRecords.length} />
          <TemplateReviewCount label={t("settings.customModels.reviewMissing")} value={missingRecords.length} />
        </div>
        <ScrollArea className="min-h-0 flex-1">
          <div className="space-y-5 p-5">
            {loading ? <p className="flex items-center gap-2 text-sm text-muted-foreground"><LoaderCircle className="size-4 animate-spin" />{t("settings.customModels.templateSearching")}</p> : null}
            {!loading && exactRecords.length > 0 ? (
              <TemplateReviewSection title={t("settings.customModels.reviewExactDescription")}>
                {exactRecords.map((record) => <TemplateReviewRow key={record.modelId} record={record} />)}
              </TemplateReviewSection>
            ) : null}
            {!loading && similarRecords.length > 0 ? (
              <TemplateReviewSection title={t("settings.customModels.reviewSimilarDescription")}>
                {similarRecords.map((record) => {
                  const suggestion = record.suggestions[0]
                  return (
                    <TemplateReviewRow
                      actions={suggestion ? (
                        <div className="flex gap-2">
                          <Button disabled={saving} onClick={() => void saveDecisions([{ modelId: record.modelId, decision: appliedTemplateDecision(suggestion) }])} size="sm" type="button">{t("settings.customModels.templateApply")}</Button>
                          <Button disabled={saving} onClick={() => void saveDecisions([{ modelId: record.modelId, decision: rejectedTemplateDecision(suggestion) }])} size="sm" type="button" variant="outline">{t("settings.customModels.templateKeepDefaults")}</Button>
                        </div>
                      ) : null}
                      key={record.modelId}
                      record={record}
                    />
                  )
                })}
              </TemplateReviewSection>
            ) : null}
            {!loading && missingRecords.length > 0 ? (
              <TemplateReviewSection title={t("settings.customModels.reviewMissingDescription")}>
                {missingRecords.map((record) => <TemplateReviewRow key={record.modelId} record={record} />)}
              </TemplateReviewSection>
            ) : null}
            {!loading && undecidedRecords.length === 0 ? <p className="py-12 text-center text-sm text-muted-foreground">{t("settings.customModels.reviewComplete")}</p> : null}
            {error ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{error}</p> : null}
          </div>
        </ScrollArea>
        <DialogFooter className="border-t px-5 py-3">
          <Button onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.close")}</Button>
          {exactRecords.length > 0 ? (
            <Button disabled={saving || loading} onClick={() => void saveDecisions(exactRecords.map((record) => ({
              modelId: record.modelId,
              decision: appliedTemplateDecision(record.suggestions[0]),
            })))} type="button">
              {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : <CheckCheck className="mr-2 size-4" />}
              {t("settings.customModels.applyAllExact").replace("{count}", String(exactRecords.length))}
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// TemplateReviewCount renders one compact suggestion group count.
// TemplateReviewCount 渲染一个紧凑的建议分组计数。
function TemplateReviewCount(props: { label: string; value: number }) {
  return <div className="rounded-lg border p-3"><p className="text-xs text-muted-foreground">{props.label}</p><p className="mt-1 text-lg font-semibold">{props.value}</p></div>
}

// TemplateReviewSection groups rows under one human-readable review instruction.
// TemplateReviewSection 将多行按一项人类可读审查说明分组。
function TemplateReviewSection(props: { children: React.ReactNode; title: string }) {
  return <section className="space-y-2"><h3 className="text-sm font-semibold">{props.title}</h3><div className="space-y-2">{props.children}</div></section>
}

// TemplateReviewRow renders one model and its strongest offline suggestion.
// TemplateReviewRow 渲染一个模型及其最强离线建议。
function TemplateReviewRow(props: { actions?: React.ReactNode; record: PublicAiCustomModelTemplateSuggestionsRecord }) {
  const { t } = useI18n()
  const suggestion = props.record.suggestions[0]
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
      <div className="min-w-0">
        <p className="truncate font-mono text-sm font-medium">{props.record.modelId}</p>
        <p className="mt-1 text-xs text-muted-foreground">
          {suggestion
            ? `${suggestion.template_key} · ${suggestion.context_limit.toLocaleString()} Token · ${t(`settings.customModels.templateMatch.${suggestion.match_kind}`)}`
            : t("settings.customModels.templateDefaultFallback")}
        </p>
      </div>
      {props.actions}
    </div>
  )
}

// appliedTemplateDecision snapshots one immutable suggestion after explicit review.
// appliedTemplateDecision 在显式审查后快照一项不可变建议。
function appliedTemplateDecision(suggestion: PublicAiCommonModelTemplateSuggestion): PublicAiCustomModelTemplateDecision {
  return {
    decision: "applied",
    selection: {
      template_key: suggestion.template_key,
      template_revision: suggestion.template_revision,
      context_limit: suggestion.context_limit,
      supports_text_input: suggestion.supports_text_input,
      supports_text_output: suggestion.supports_text_output,
      input_resources: suggestion.input_resources,
      output_resources: suggestion.output_resources,
    },
  }
}

// rejectedTemplateDecision records an explicit choice to keep provider defaults.
// rejectedTemplateDecision 记录一项显式保留供应商默认值的选择。
function rejectedTemplateDecision(suggestion: PublicAiCommonModelTemplateSuggestion): PublicAiCustomModelTemplateDecision {
  return {
    decision: "rejected",
    template_key: suggestion.template_key,
    template_revision: suggestion.template_revision,
  }
}

// DiscoverySummary renders provider-wide account status without flattening failures into one error.
// DiscoverySummary 渲染供应商级账号状态且不把失败压平成一个错误。
function DiscoverySummary(props: {
  onRefresh: () => void
  onRefreshConnection: (connectionId: string) => void
  onReviewTemplates: () => void
  providerId: string
  snapshot: PublicAiSettingsSnapshot | null
}) {
  const { t } = useI18n()
  const connections = props.snapshot?.connections.filter((connection) => connection.provider_id === props.providerId) ?? []
  const states = connections.map((connection) => props.snapshot?.catalogStatuses[connection.id]?.state ?? "not_checked")
  const failed = states.filter((state) => !["ready", "empty"].includes(state)).length
  const modelIds = new Set(connections.flatMap((connection) => props.snapshot?.sourceCatalogs[connection.id]?.models.map((model) => model.id) ?? []))
  const latestObservation = connections.reduce((latest, connection) => (
    Math.max(latest, props.snapshot?.catalogStatuses[connection.id]?.observed_at_ms ?? 0)
  ), 0)
  return (
    <div className="rounded-xl border p-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">{t("settings.customModels.discoveryTitle")}</h3>
          <p className="text-sm text-muted-foreground">{t("settings.customModels.discoverySummary").replace("{accounts}", String(connections.length)).replace("{models}", String(modelIds.size)).replace("{failed}", String(failed))}</p>
          <p className="mt-1 text-xs text-muted-foreground">{t("settings.customModels.lastSynchronized").replace("{time}", formatObservationTime(latestObservation, t("settings.customModels.notSynchronized")))}</p>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button onClick={props.onReviewTemplates} size="sm" type="button" variant="outline"><Sparkles className="mr-2 size-4" />{t("settings.customModels.reviewTemplates")}</Button>
          <Button onClick={props.onRefresh} size="sm" type="button" variant="outline"><RefreshCw className="mr-2 size-4" />{t("settings.customModels.refreshAll")}</Button>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {connections.map((connection) => {
          const state = props.snapshot?.catalogStatuses[connection.id]?.state ?? "not_checked"
          return <Badge key={connection.id} variant={state === "ready" ? "secondary" : "outline"}>{connection.display_name} · {t(DISCOVERY_STATE_LABEL_KEYS[state])}</Badge>
        })}
      </div>
      {connections.length > 0 ? (
        <Accordion className="mt-3" collapsible type="single">
          <AccordionItem className="border-b-0" value="accounts">
            <AccordionTrigger className="py-2 text-sm">{t("settings.customModels.discoveryDetails")}</AccordionTrigger>
            <AccordionContent className="space-y-2">
              {connections.map((connection) => {
                const status = props.snapshot?.catalogStatuses[connection.id]
                // sourceCatalog is the exact memory-only discovery evidence for this credential.
                // sourceCatalog 是此凭据对应的精确内存发现证据。
                const sourceCatalog = props.snapshot?.sourceCatalogs[connection.id]
                return (
                  <div className="rounded-lg border p-3" key={connection.id}>
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">{connection.display_name}</p>
                        <p className="text-xs text-muted-foreground">
                          {t(DISCOVERY_STATE_LABEL_KEYS[status?.state ?? "not_checked"])}
                          {" · "}{t("settings.customModels.discoveryModelCount").replace("{count}", String(status?.model_count ?? 0))}
                          {sourceCatalog ? <>{" · "}{t("settings.customModels.discoveryPageCount").replace("{count}", String(sourceCatalog.page_count))}</> : null}
                          {" · "}{formatObservationTime(status?.observed_at_ms ?? 0, t("settings.customModels.notSynchronized"))}
                        </p>
                        {sourceCatalog ? <p className="truncate font-mono text-[11px] text-muted-foreground">{t("settings.customModels.discoverySchema").replace("{revision}", sourceCatalog.discovery_schema_revision)}</p> : null}
                      </div>
                      <Button onClick={() => props.onRefreshConnection(connection.id)} size="sm" type="button" variant="outline">
                        <RefreshCw className="mr-2 size-3.5" />{t("settings.customModels.refreshAccount")}
                      </Button>
                    </div>
                    {status?.message ? <pre className="mt-2 max-h-28 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-2 text-xs text-muted-foreground">{status.message}</pre> : null}
                  </div>
                )
              })}
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      ) : null}
    </div>
  )
}

// ModelDetailHeader renders immutable identity and one provider-model access switch.
// ModelDetailHeader 渲染不可变身份与一个供应商模型访问开关。
function ModelDetailHeader(props: { onEnabledChange: (enabled: boolean) => void; row: CustomProviderModelRow }) {
  const { t } = useI18n()
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <h3 className="truncate text-lg font-semibold">{props.row.displayName}</h3>
        <p className="truncate font-mono text-xs text-muted-foreground">{props.row.id}</p>
      </div>
      <label className="flex items-center gap-2 text-sm"><span>{t("settings.customModels.enabled")}</span><Switch checked={props.row.enabled} onCheckedChange={props.onEnabledChange} /></label>
    </div>
  )
}

// ResolvedCapabilitySummary explains final basic capabilities and their strongest visible source.
// ResolvedCapabilitySummary 说明最终基础能力及其最强可见来源。
function CapabilityAccountSelector(props: {
  connectionId: string
  onConnectionChange: (connectionId: string) => void
  row: CustomProviderModelRow
}) {
  const { t } = useI18n()
  if (props.row.accounts.length === 0) {
    return <Alert><CircleAlert className="size-4" /><AlertTitle>{t("settings.customModels.noAccounts")}</AlertTitle><AlertDescription>{t("settings.customModels.noAccountsDescription")}</AlertDescription></Alert>
  }
  return (
    <label className="grid gap-2 text-sm font-medium">
      {t("settings.customModels.capabilityAccount")}
      <Select onValueChange={props.onConnectionChange} value={props.connectionId}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>
          {props.row.accounts.map((account) => (
            <SelectItem key={account.connection.id} value={account.connection.id}>
              {account.connection.display_name} · {account.connection.pending_validation ? t("settings.customModels.pendingValidation") : t(DISCOVERY_STATE_LABEL_KEYS[account.status?.state ?? "not_checked"])}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}

// ResolvedCapabilitySummary explains final basic capabilities for one exact account.
// ResolvedCapabilitySummary 说明一个精确账号的最终基础能力。
function ResolvedCapabilitySummary(props: {
  account?: CustomProviderModelAccountState
  defaults: { context_limit: number; output_limit: number } | null
  row: CustomProviderModelRow
}) {
  const { t } = useI18n()
  const capabilities = props.account?.effectiveModel?.capabilities ?? props.row.effectiveModel?.capabilities
  const context = capabilities?.context_limit ?? props.defaults?.context_limit
  const output = capabilities?.output_limit ?? props.defaults?.output_limit
  const inputHasText = resolvedTextSupport(props.row, props.account, "input")
  const outputHasText = resolvedTextSupport(props.row, props.account, "output")
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <CapabilityFact label={t("settings.customModels.contextLimit")} source={capabilitySource(props.row, props.account, "context_limit", t)} value={context?.toLocaleString() ?? "—"} />
      <CapabilityFact label={t("settings.customModels.outputLimit")} source={capabilitySource(props.row, props.account, "output_limit", t)} value={output?.toLocaleString() ?? "—"} />
      <CapabilityFact label={t("settings.customModels.input")} source={capabilitySource(props.row, props.account, "input_resources", t)} value={resourceSummary(capabilities?.input_resources ?? [], inputHasText, t)} />
      <CapabilityFact label={t("settings.customModels.output")} source={capabilitySource(props.row, props.account, "output_resources", t)} value={resourceSummary(capabilities?.output_resources ?? [], outputHasText, t)} />
    </div>
  )
}

// CapabilityFact renders one resolved value together with a provenance badge.
// CapabilityFact 渲染一个解析值及其来源徽标。
function CapabilityFact(props: { label: string; source: string; value: string }) {
  return <div className="rounded-lg border p-3"><div className="flex items-center justify-between gap-2"><span className="text-xs text-muted-foreground">{props.label}</span><Badge variant="outline">{props.source}</Badge></div><p className="mt-2 font-medium">{props.value}</p></div>
}

// AccountAvailabilityEditor renders exact source/effective states and explicit manual overrides.
// AccountAvailabilityEditor 渲染精确源/有效状态与显式手工覆盖。
function AccountAvailabilityEditor(props: {
  onAccountSelected: (connectionId: string) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  providerId: string
  row: CustomProviderModelRow
  selectedConnectionId: string | null
}) {
  const { t } = useI18n()
  const [savingConnectionId, setSavingConnectionId] = React.useState<string | null>(null)
  const [validationTarget, setValidationTarget] = React.useState<CustomProviderModelAccountState | null>(null)
  const [validatingConnectionId, setValidatingConnectionId] = React.useState<string | null>(null)
  const [validationResult, setValidationResult] = React.useState<string | null>(null)
  const [availabilityConfirmation, setAvailabilityConfirmation] = React.useState<{
    account: CustomProviderModelAccountState
    value: "inherit" | "enabled" | "disabled"
  } | null>(null)
  const [error, setError] = React.useState<string | null>(null)

  // saveAvailability persists one tri-state account-model availability decision.
  // saveAvailability 持久化一个三态账号模型可用性决定。
  const saveAvailability = React.useCallback(async (
    connectionId: string,
    value: "inherit" | "enabled" | "disabled",
  ) => {
    setSavingConnectionId(connectionId)
    setError(null)
    try {
      props.onSaved(await savePublicAiCustomModelConnectionAvailability({
        connectionId,
        modelId: props.row.id,
        enabled: value === "inherit" ? undefined : value === "enabled",
      }))
      setAvailabilityConfirmation(null)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSavingConnectionId(null)
    }
  }, [props])

  // validateAccount sends one user-confirmed minimal paid request before releasing a pending account.
  // validateAccount 在解除待验证账号隔离前发送一次用户确认的最小付费请求。
  const validateAccount = React.useCallback(async () => {
    if (!validationTarget) {
      return
    }
    setValidatingConnectionId(validationTarget.connection.id)
    setError(null)
    setValidationResult(null)
    try {
      const result = await validatePublicAiCustomModel({
        providerId: props.providerId,
        connectionId: validationTarget.connection.id,
        modelId: props.row.id,
      })
      props.onSaved(await loadPublicAiSettings())
      setValidationResult(
        t("settings.customModels.validationSucceeded")
          .replace("{account}", validationTarget.connection.display_name)
          .replace("{response}", result.responseText),
      )
      setValidationTarget(null)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setValidatingConnectionId(null)
    }
  }, [props, t, validationTarget])

  return (
    <div className="space-y-3">
      {props.row.accounts.map((account) => (
        <div className="grid gap-3 rounded-lg border p-3 sm:grid-cols-[minmax(0,1fr)_auto]" key={account.connection.id}>
          <div className="min-w-0">
            <p className="truncate font-medium">{account.connection.display_name}</p>
            <div className="mt-1 flex flex-wrap gap-1">
              <Badge variant="outline">{t(DISCOVERY_STATE_LABEL_KEYS[account.status?.state ?? "not_checked"])}</Badge>
              {account.connection.pending_validation ? <Badge variant="destructive">{t("settings.customModels.pendingValidation")}</Badge> : null}
              <Badge variant={accountActuallyDiscoveredModel(account) ? "secondary" : "outline"}>{accountActuallyDiscoveredModel(account) ? t("settings.customModels.accountReturned") : t("settings.customModels.accountNotReturned")}</Badge>
              <Badge variant={accountExposesModel(account) ? "secondary" : "outline"}>{accountExposesModel(account) ? t("settings.customModels.accountAvailable") : t("settings.customModels.accountUnavailable")}</Badge>
            </div>
          </div>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button
              onClick={() => props.onAccountSelected(account.connection.id)}
              size="sm"
              type="button"
              variant={props.selectedConnectionId === account.connection.id ? "secondary" : "outline"}
            >
              {t("settings.customModels.viewAccountCapabilities")}
            </Button>
            <Button
              disabled={!accountExposesModel(account) || validatingConnectionId !== null}
              onClick={() => {
                setError(null)
                setValidationResult(null)
                setValidationTarget(account)
              }}
              size="sm"
              type="button"
              variant={account.connection.pending_validation ? "default" : "outline"}
            >
              {validatingConnectionId === account.connection.id ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("settings.customModels.validateModel")}
            </Button>
            {props.row.manualDefinition ? (
              <Select
                disabled={savingConnectionId === account.connection.id}
                onValueChange={(value) => {
                  setError(null)
                  setAvailabilityConfirmation({
                    account,
                    value: value as "inherit" | "enabled" | "disabled",
                  })
                }}
                value={account.availabilityOverride === undefined ? "inherit" : account.availabilityOverride ? "enabled" : "disabled"}
              >
                <SelectTrigger className="w-36"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="inherit">{t("settings.customModels.accountInherit")}</SelectItem>
                  <SelectItem value="enabled">{t("settings.customModels.accountForceEnable")}</SelectItem>
                  <SelectItem value="disabled">{t("settings.customModels.accountForceDisable")}</SelectItem>
                </SelectContent>
              </Select>
            ) : null}
          </div>
        </div>
      ))}
      {validationResult ? <Alert><AlertTitle>{t("settings.customModels.validationComplete")}</AlertTitle><AlertDescription>{validationResult}</AlertDescription></Alert> : null}
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <AlertDialog
        onOpenChange={(open) => {
          if (!open && validatingConnectionId === null) {
            setValidationTarget(null)
          }
        }}
        open={validationTarget !== null}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.validateModelTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.customModels.validateModelDescription")
                .replace("{model}", props.row.displayName)
                .replace("{account}", validationTarget?.connection.display_name ?? "")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {error ? <p className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">{error}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={validatingConnectionId !== null}>{t("memory.common.cancel")}</AlertDialogCancel>
            <Button disabled={validatingConnectionId !== null} onClick={() => void validateAccount()} type="button">
              {validatingConnectionId !== null ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("settings.customModels.confirmValidation")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <CustomProviderChangeSummaryDialog
        description={t("settings.customModels.changeSummaryDescription").replace("{model}", props.row.displayName)}
        error={error}
        items={availabilityConfirmation ? [
          { label: t("settings.customModels.summaryModel"), value: props.row.displayName },
          { label: t("settings.customModels.summaryAccount"), value: availabilityConfirmation.account.connection.display_name },
          {
            label: t("settings.customModels.summaryAvailability"),
            value: t(availabilityConfirmation.value === "inherit"
              ? "settings.customModels.accountInherit"
              : availabilityConfirmation.value === "enabled"
                ? "settings.customModels.accountForceEnable"
                : "settings.customModels.accountForceDisable"),
          },
          ...(availabilityConfirmation.account.connection.pending_validation ? [{
            label: t("settings.customModels.summaryRoutingState"),
            value: t("settings.customModels.pendingValidation"),
            warning: true,
          }] : []),
        ] : []}
        onConfirm={() => {
          if (availabilityConfirmation) {
            void saveAvailability(
              availabilityConfirmation.account.connection.id,
              availabilityConfirmation.value,
            )
          }
        }}
        onOpenChange={(open) => {
          if (!open && savingConnectionId === null) {
            setAvailabilityConfirmation(null)
          }
        }}
        open={availabilityConfirmation !== null}
        saving={savingConnectionId !== null}
      />
    </div>
  )
}

// CapabilityOverridesEditor stores only explicit differences from discovery, template, and defaults.
// CapabilityOverridesEditor 仅保存相对于发现、模板与默认值的显式差异。
function CapabilityOverridesEditor(props: { onSaved: (snapshot: PublicAiSettingsSnapshot) => void; providerId: string; row: CustomProviderModelRow }) {
  const { t } = useI18n()
  const [draft, setDraft] = React.useState<PublicAiModelCapabilityOverrides>(props.row.capabilityOverrides ?? {})
  const [saving, setSaving] = React.useState(false)
  const [confirmationOpen, setConfirmationOpen] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  React.useEffect(() => setDraft(props.row.capabilityOverrides ?? {}), [props.row.capabilityOverrides, props.row.id])

  // save writes one strict optional-field difference map.
  // save 写入一个严格的可选字段差异映射。
  const save = React.useCallback(async () => {
    setSaving(true)
    setError(null)
    try {
      const empty = Object.keys(draft).length === 0
      props.onSaved(await savePublicAiModelCapabilityOverrides({
        providerId: props.providerId,
        modelId: props.row.id,
        overrides: empty ? undefined : draft,
      }))
      setConfirmationOpen(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [draft, props])

  // summaryValue explains every explicit capability difference before persistence.
  // summaryValue 在持久化前说明每一项显式能力差异。
  const summaryValue = [
    draft.context_limit !== undefined
      ? `${t("settings.customModels.contextLimit")}: ${draft.context_limit.toLocaleString()}`
      : null,
    draft.output_limit !== undefined
      ? `${t("settings.customModels.outputLimit")}: ${draft.output_limit.toLocaleString()}`
      : null,
    draft.input_resources !== undefined
      ? `${t("settings.customModels.input")}: ${draft.input_resources.length > 0 ? draft.input_resources.map((resource) => t(RESOURCE_LABEL_KEYS[resource])).join(", ") : t("settings.customModels.textOnly")}`
      : null,
    draft.output_resources !== undefined
      ? `${t("settings.customModels.output")}: ${draft.output_resources.length > 0 ? draft.output_resources.map((resource) => t(RESOURCE_LABEL_KEYS[resource])).join(", ") : t("settings.customModels.textOnly")}`
      : null,
  ].filter((value): value is string => value !== null).join(" · ") || t("settings.customModels.summaryRemoveOverrides")

  return (
    <div className="space-y-4 rounded-lg border p-4">
      <p className="text-sm text-muted-foreground">{t("settings.modelCapabilities.overlayHint")}</p>
      <OptionalNumberOverride label={t("settings.customModels.contextLimit")} onChange={(value) => setDraft((current) => ({ ...current, context_limit: value }))} onEnabledChange={(enabled) => setDraft((current) => withoutOptionalField(current, "context_limit", enabled ? props.row.effectiveModel?.capabilities.context_limit ?? 200_000 : undefined))} value={draft.context_limit} />
      <OptionalNumberOverride label={t("settings.customModels.outputLimit")} onChange={(value) => setDraft((current) => ({ ...current, output_limit: value }))} onEnabledChange={(enabled) => setDraft((current) => withoutOptionalField(current, "output_limit", enabled ? props.row.effectiveModel?.capabilities.output_limit ?? 32_768 : undefined))} value={draft.output_limit} />
      <OptionalResourceOverride field="input_resources" label={t("settings.customModels.input")} onChange={setDraft} value={draft.input_resources} />
      <OptionalResourceOverride field="output_resources" label={t("settings.customModels.output")} onChange={setDraft} value={draft.output_resources} />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <div className="flex justify-end gap-2"><Button onClick={() => setDraft({})} type="button" variant="outline">{t("settings.modelCapabilities.restore")}</Button><Button disabled={saving} onClick={() => setConfirmationOpen(true)} type="button">{saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t("settings.customModels.saveCapabilityOverrides")}</Button></div>
      <CustomProviderChangeSummaryDialog
        description={t("settings.customModels.changeSummaryDescription").replace("{model}", props.row.displayName)}
        error={error}
        items={[
          { label: t("settings.customModels.summaryModel"), value: props.row.displayName },
          { label: t("settings.customModels.summaryCapabilityOverrides"), value: summaryValue },
        ]}
        onConfirm={() => void save()}
        onOpenChange={(open) => {
          if (!saving) setConfirmationOpen(open)
        }}
        open={confirmationOpen}
        saving={saving}
      />
    </div>
  )
}

// OptionalNumberOverride edits one optional positive integer difference.
// OptionalNumberOverride 编辑一个可选的正整数差异。
function OptionalNumberOverride(props: { label: string; onChange: (value: number) => void; onEnabledChange: (enabled: boolean) => void; value?: number }) {
  return <div className="grid gap-2 sm:grid-cols-[auto_minmax(0,1fr)] sm:items-center"><label className="flex items-center gap-2 text-sm"><Checkbox checked={props.value !== undefined} onCheckedChange={(checked) => props.onEnabledChange(Boolean(checked))} />{props.label}</label><Input disabled={props.value === undefined} inputMode="numeric" onChange={(event) => props.onChange(Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))} value={props.value ?? ""} /></div>
}

// OptionalResourceOverride edits one optional complete resource-set replacement.
// OptionalResourceOverride 编辑一个可选的完整资源集合替换。
function OptionalResourceOverride(props: { field: "input_resources" | "output_resources"; label: string; onChange: React.Dispatch<React.SetStateAction<PublicAiModelCapabilityOverrides>>; value?: PublicAiResourceKind[] }) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-3">
      <legend className="px-1 text-sm font-medium"><label className="flex items-center gap-2"><Checkbox checked={props.value !== undefined} onCheckedChange={(checked) => props.onChange((current) => withoutOptionalField(current, props.field, checked ? [] : undefined))} />{props.label}</label></legend>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        {RESOURCE_KINDS.map((resource) => <label className="flex items-center gap-2 text-sm" key={resource}><Checkbox checked={props.value?.includes(resource) ?? false} disabled={props.value === undefined} onCheckedChange={(checked) => props.onChange((current) => ({ ...current, [props.field]: checked ? [...(props.value ?? []), resource] : (props.value ?? []).filter((item) => item !== resource) }))} />{t(RESOURCE_LABEL_KEYS[resource])}</label>)}
      </div>
    </fieldset>
  )
}

// AdvancedModelEditor edits manual identity metadata and protocol-specific exact declarations.
// AdvancedModelEditor 编辑手工身份元数据与协议专属精确声明。
function AdvancedModelEditor(props: {
  manualDefinition?: PublicAiCustomModelDefinition
  onCreateOverlay: () => Promise<void>
  onRemove: () => void
  onSave: () => void
  onUpdate: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void
  parameterText: string
  projectionText: string
  setParameterText: (value: string) => void
  setProjectionText: (value: string) => void
  saving: boolean
}) {
  const { t } = useI18n()
  if (!props.manualDefinition) {
    return <Alert><SlidersHorizontal className="size-4" /><AlertTitle>{t("settings.customModels.advancedUnavailable")}</AlertTitle><AlertDescription className="space-y-3"><p>{t("settings.customModels.advancedCreateHint")}</p><Button onClick={() => void props.onCreateOverlay()} size="sm" type="button">{t("settings.customModels.advancedCreate")}</Button></AlertDescription></Alert>
  }
  const model = props.manualDefinition
  return (
    <div className="space-y-4">
      <label className="grid gap-2 text-sm font-medium">{t("settings.customModels.displayName")}<Input onChange={(event) => props.onUpdate((current) => ({ ...current, display_name: event.target.value }))} value={model.display_name} /></label>
      <div className="grid gap-3 sm:grid-cols-2">
        <CapabilitySelect label={t("settings.customModels.toolCalling")} onChange={(value) => props.onUpdate((current) => ({ ...current, capabilities: { ...current.capabilities, tool_calling: value } }))} value={model.capabilities.tool_calling as ReasoningSupport} />
        <CapabilitySelect label={t("settings.customModels.reasoning")} onChange={(value) => props.onUpdate((current) => ({ ...current, capabilities: { ...current.capabilities, reasoning: value } }))} value={model.capabilities.reasoning as ReasoningSupport} />
      </div>
      <ReasoningControlEditor model={model} onUpdate={props.onUpdate} />
      <Accordion collapsible type="single"><AccordionItem value="json"><AccordionTrigger>{t("settings.customModels.advanced")}</AccordionTrigger><AccordionContent className="space-y-4"><label className="grid gap-2 text-sm font-medium">{t("settings.customModels.parameters")}<Textarea className="min-h-32 font-mono text-xs" onChange={(event) => props.setParameterText(event.target.value)} spellCheck={false} value={props.parameterText} /></label><label className="grid gap-2 text-sm font-medium">{t("settings.customModels.projection")}<Textarea className="min-h-52 font-mono text-xs" onChange={(event) => props.setProjectionText(event.target.value)} spellCheck={false} value={props.projectionText} /></label></AccordionContent></AccordionItem></Accordion>
      <div className="flex flex-wrap justify-between gap-2"><Button className="text-destructive hover:text-destructive" onClick={props.onRemove} type="button" variant="outline"><Trash2 className="mr-2 size-4" />{t("settings.customModels.remove")}</Button><Button disabled={props.saving} onClick={props.onSave} type="button">{t("settings.customModels.saveAdvanced")}</Button></div>
    </div>
  )
}

// ReasoningControlEditor edits the exact manual-model reasoning behavior and levels.
// ReasoningControlEditor 编辑精确的手工模型思考行为与档位。
function ReasoningControlEditor(props: { model: PublicAiCustomModelDefinition; onUpdate: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void }) {
  const { t } = useI18n()
  const controls = Object.keys(REASONING_CONTROL_LABEL_KEYS) as Array<keyof typeof REASONING_CONTROL_LABEL_KEYS>
  return <div className="space-y-3"><label className="grid gap-2 text-sm font-medium">{t("settings.customModels.reasoningControl")}<Select onValueChange={(value) => props.onUpdate((model) => ({ ...model, capabilities: { ...model.capabilities, reasoning_control: value } }))} value={props.model.capabilities.reasoning_control ?? "unknown"}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{controls.map((value) => <SelectItem key={value} value={value}>{t(REASONING_CONTROL_LABEL_KEYS[value])}</SelectItem>)}</SelectContent></Select></label><div className="flex flex-wrap gap-2">{REASONING_EFFORTS.map((effort) => <label className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm" key={effort}><Checkbox checked={(props.model.capabilities.reasoning_efforts ?? []).includes(effort)} onCheckedChange={(checked) => props.onUpdate((model) => ({ ...model, capabilities: { ...model.capabilities, reasoning_efforts: checked ? [...(model.capabilities.reasoning_efforts ?? []), effort] : (model.capabilities.reasoning_efforts ?? []).filter((item) => item !== effort) } }))} />{t(REASONING_EFFORT_LABEL_KEYS[effort])}</label>)}</div></div>
}

// CapabilitySelect renders the closed provider-neutral support fidelity values.
// CapabilitySelect 渲染封闭的供应商无关支持保真度值。
function CapabilitySelect(props: { label: string; onChange: (value: ReasoningSupport) => void; value: ReasoningSupport }) {
  const { t } = useI18n()
  const values = Object.keys(SUPPORT_LABEL_KEYS) as ReasoningSupport[]
  return <label className="grid gap-2 text-sm font-medium">{props.label}<Select onValueChange={(value) => props.onChange(value as ReasoningSupport)} value={props.value}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent>{values.map((value) => <SelectItem key={value} value={value}>{t(SUPPORT_LABEL_KEYS[value])}</SelectItem>)}</SelectContent></Select></label>
}

// ManualModelDialog asks for a real model ID before any model object is created.
// ManualModelDialog 在创建任何模型对象前要求输入真实模型 ID。
function ManualModelDialog(props: {
  defaults: PublicAiCustomProviderModelDefaults
  existingIds: Set<string>
  onOpenChange: (open: boolean) => void
  onSaved: (
    model: PublicAiCustomModelDefinition,
    decision?: PublicAiCustomModelTemplateDecision,
  ) => Promise<void>
  open: boolean
}) {
  const { t } = useI18n()
  const [modelId, setModelId] = React.useState("")
  const [displayName, setDisplayName] = React.useState("")
  const [suggestions, setSuggestions] = React.useState<PublicAiCommonModelTemplateSuggestion[]>([])
  const [choice, setChoice] = React.useState("defaults")
  const [suggestionLoading, setSuggestionLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  React.useEffect(() => {
    if (!props.open) {
      setModelId("")
      setDisplayName("")
      setSuggestions([])
      setChoice("defaults")
      setSuggestionLoading(false)
      setError(null)
      return
    }
    setSuggestions([])
    setChoice("defaults")
    setError(null)
    if (!modelId.trim()) {
      setSuggestionLoading(false)
      return
    }
    setSuggestionLoading(true)
    let active = true
    const timer = window.setTimeout(() => {
      void getPublicAiCustomModelTemplateSuggestions(modelId.trim())
        .then((result) => {
          if (active) setSuggestions(result)
        })
        .catch((reason) => {
          if (active) setError(errorMessage(reason))
        })
        .finally(() => {
          if (active) setSuggestionLoading(false)
        })
    }, 400)
    return () => { active = false; window.clearTimeout(timer) }
  }, [modelId, props.open])

  // submit persists the real identity first and then applies the explicit local template choice.
  // submit 先持久化真实身份，再应用明确的本地模板选择。
  const submit = React.useCallback(async () => {
    const id = modelId.trim()
    if (!id) { setError(t("settings.customModels.modelIdRequired")); return }
    if (props.existingIds.has(id)) { setError(t("settings.customModels.duplicateId").replace("{model}", id)); return }
    const selected = suggestions.find((suggestion) => suggestion.template_key === choice)
    const firstSuggestion = suggestions[0]
    const decision = selected ? { decision: "applied" as const, selection: { template_key: selected.template_key, template_revision: selected.template_revision, context_limit: selected.context_limit, supports_text_input: selected.supports_text_input, supports_text_output: selected.supports_text_output, input_resources: selected.input_resources, output_resources: selected.output_resources } } : firstSuggestion ? { decision: "rejected" as const, template_key: firstSuggestion.template_key, template_revision: firstSuggestion.template_revision } : undefined
    setSaving(true)
    setError(null)
    try { await props.onSaved(newManualModel(id, displayName.trim() || id), decision) } catch (reason) { setError(errorMessage(reason)) } finally { setSaving(false) }
  }, [choice, displayName, modelId, props, suggestions, t])

  const selectedSuggestion = suggestions.find((suggestion) => suggestion.template_key === choice)
  const summarySource = selectedSuggestion?.template_key ?? t("settings.customModels.templateKeepDefaults")

  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("settings.customModels.addManual")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.manualDescription")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <label className="grid gap-2 text-sm font-medium">
            {t("settings.customModels.modelId")}
            <Input autoFocus onChange={(event) => setModelId(event.target.value)} value={modelId} />
          </label>
          <label className="grid gap-2 text-sm font-medium">
            {t("settings.customModels.displayName")}
            <Input onChange={(event) => setDisplayName(event.target.value)} placeholder={modelId.trim()} value={displayName} />
          </label>
          <div className="rounded-lg border p-3">
            <p className="mb-3 text-sm font-medium">{t("settings.customModels.templateChoice")}</p>
            {suggestionLoading ? (
              <p className="mb-3 flex items-center gap-2 text-xs text-muted-foreground">
                <LoaderCircle className="size-3.5 animate-spin" />
                {t("settings.customModels.templateSearching")}
              </p>
            ) : null}
            <RadioGroup onValueChange={setChoice} value={choice}>
              <label className="flex items-start gap-3 rounded-md p-2 hover:bg-muted">
                <RadioGroupItem value="defaults" />
                <span>
                  <span className="block text-sm font-medium">{t("settings.customModels.templateKeepDefaults")}</span>
                  <span className="block text-xs text-muted-foreground">{t("settings.customModels.templateDefaultFallback")}</span>
                </span>
              </label>
              {suggestions.map((suggestion) => (
                <label className="flex items-start gap-3 rounded-md p-2 hover:bg-muted" key={suggestion.template_key}>
                  <RadioGroupItem value={suggestion.template_key} />
                  <span>
                    <span className="block text-sm font-medium">{suggestion.template_key}</span>
                    <span className="block text-xs text-muted-foreground">{suggestion.context_limit.toLocaleString()} Token · {suggestion.match_kind}</span>
                  </span>
                </label>
              ))}
            </RadioGroup>
          </div>
          <div className="rounded-lg border bg-muted/20 p-3">
            <p className="mb-2 text-sm font-medium">{t("settings.customModels.changeSummaryTitle")}</p>
            <dl className="grid gap-1 text-sm sm:grid-cols-[8rem_minmax(0,1fr)]">
              <dt className="text-muted-foreground">{t("settings.customModels.summaryModel")}</dt>
              <dd className="break-all font-medium">{modelId.trim() || t("settings.customModels.summaryNotEntered")}</dd>
              <dt className="text-muted-foreground">{t("settings.customModels.summaryTemplate")}</dt>
              <dd className="break-words font-medium">{summarySource}</dd>
              <dt className="text-muted-foreground">{t("settings.customModels.summaryDefaults")}</dt>
              <dd className="font-medium">
                {props.defaults.context_limit.toLocaleString()} / {props.defaults.output_limit.toLocaleString()} Token
              </dd>
            </dl>
          </div>
          {error ? <p className="text-sm text-destructive">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button>
          <Button
            disabled={shouldBlockManualModelSubmission(saving, suggestionLoading)}
            onClick={() => void submit()}
            type="button"
          >
            {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
            {t("settings.customModels.addManual")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// newManualModel creates identity and advanced behavior only; defaults and templates fill basic fields.
// newManualModel 仅创建身份与高级行为；基础字段由默认值和模板填充。
function newManualModel(modelId: string, displayName: string): PublicAiCustomModelDefinition {
  return { id: modelId, display_name: displayName, enabled: true, capabilities: { operations: ["conversation_respond"], input_resources: [], output_resources: [], evidence_revision: "user.manual-model.v2", reasoning: "unknown", reasoning_control: "unknown", reasoning_efforts: [], tool_calling: "unknown", parallel_tool_calling: "unknown", prompt_cache: { provider_default_behavior: "Follow provider defaults without host-injected cache controls.", supports_managed_explicit: false, supported_ttl_options: [] }, model_parameters: [] }, request_projection: structuredClone(EMPTY_PROJECTION) }
}

// capabilitySource resolves the strongest visible source for one account and capability field.
// capabilitySource 解析一个账号与能力字段的最强可见来源。
function capabilitySource(
  row: CustomProviderModelRow,
  account: CustomProviderModelAccountState | undefined,
  field: keyof PublicAiModelCapabilityOverrides,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (row.capabilityOverrides && field in row.capabilityOverrides) return t("settings.customModels.sourceUserOverride")
  const evidence = account?.discoveredModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  if (evidence) {
    if (field === "context_limit" && evidence.context_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "output_limit" && evidence.output_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "input_resources" && evidence.input_modalities_declared) return t("settings.customModels.sourceAccount")
    if (field === "output_resources" && evidence.output_modalities_declared) return t("settings.customModels.sourceAccount")
  }
  if (row.templateDecision?.decision === "applied" && field !== "output_limit") return t("settings.customModels.sourceTemplate")
  return t("settings.customModels.sourceProviderDefault")
}

// resolvedTextSupport resolves text modality without treating an explicit false value as missing.
// resolvedTextSupport 解析文本模态且不会把显式 false 当作缺失值。
function resolvedTextSupport(
  row: CustomProviderModelRow,
  account: CustomProviderModelAccountState | undefined,
  direction: "input" | "output",
): boolean {
  const evidence = account?.discoveredModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  const declared = direction === "input"
    ? evidence?.input_modalities_declared
    : evidence?.output_modalities_declared
  const supported = direction === "input"
    ? evidence?.supports_text_input
    : evidence?.supports_text_output
  if (declared) {
    return supported === true
  }
  if (row.templateDecision?.decision === "applied") {
    return direction === "input"
      ? row.templateDecision.selection.supports_text_input
      : row.templateDecision.selection.supports_text_output
  }
  return true
}

// resourceSummary formats explicit text and non-text resource modalities.
// resourceSummary 格式化明确的文本与非文本资源模态。
function resourceSummary(
  resources: PublicAiResourceKind[],
  includesText: boolean,
  t: ReturnType<typeof useI18n>["t"],
): string {
  const values = [
    ...(includesText ? [t("settings.models.modality.text")] : []),
    ...resources.map((resource) => t(RESOURCE_LABEL_KEYS[resource])),
  ]
  return values.length > 0 ? values.join(" · ") : "—"
}

// formatObservationTime renders process-local discovery time without inventing a timestamp.
// formatObservationTime 渲染进程内发现时间且不会虚构时间戳。
function formatObservationTime(observedAtMs: number, fallback: string): string {
  return observedAtMs > 0 ? new Date(observedAtMs).toLocaleString() : fallback
}

// withoutOptionalField sets or removes one exact optional field without multi-source guessing.
// withoutOptionalField 设置或移除一个精确可选字段且不进行多来源猜测。
function withoutOptionalField<Field extends keyof PublicAiModelCapabilityOverrides>(current: PublicAiModelCapabilityOverrides, field: Field, value: PublicAiModelCapabilityOverrides[Field]): PublicAiModelCapabilityOverrides {
  const next = { ...current }
  if (value === undefined) delete next[field]
  else next[field] = value
  return next
}

// cloneCustomModel creates an isolated editable copy of one persisted model definition.
// cloneCustomModel 创建一个持久化模型定义的隔离可编辑副本。
function cloneCustomModel(model: PublicAiCustomModelDefinition): PublicAiCustomModelDefinition { return structuredClone(model) }

// errorMessage normalizes rejected service and JSON errors for display.
// errorMessage 规范化服务拒绝与 JSON 错误以供显示。
function errorMessage(reason: unknown): string { return reason instanceof Error ? reason.message : String(reason) }
