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
  RotateCcw,
  Search,
  Sparkles,
  Trash2,
  Users,
  X,
} from "lucide-react"

import { CustomModelTemplateCard } from "@/components/features/settings/custom-model-template-card"
import {
  buildCapabilityOverridePatch,
  ContextWindowPolicyEditor,
  ReasoningProjectionEditor,
} from "@/components/features/settings/public-ai-model-capability-dialog"
import { CustomProviderChangeSummaryDialog } from "@/components/features/settings/custom-provider-change-summary-dialog"
import {
  advancedModelTextDraftFromDefinition,
  applyAdvancedModelTextDrafts,
  type AdvancedModelTextDraft,
} from "@/components/features/settings/custom-provider-model-drafts"
import { CustomProviderModelDefaultsCard } from "@/components/features/settings/custom-provider-model-defaults-card"
import {
  directCustomModelSettingsDraft,
  directProviderModelSettingsEqual,
  OPENAI_DIRECT_REASONING_EFFORTS,
  resolveDirectCustomProviderDefaults,
} from "@/components/features/settings/direct-provider-model-settings"
import {
  accountActuallyDiscoveredModel,
  accountExposesModel,
  accountIncludesModel,
  buildCustomProviderModelRows,
  mergeCustomModelProtocolPreferences,
  shouldBlockManualModelSubmission,
  shouldReplaceManualModelDrafts,
  type CustomProviderModelAccountState,
  type CustomProviderModelRow,
} from "@/components/features/settings/custom-provider-models-state"
import {
  customProtocolProfileId,
  customProtocolsFromServiceEntries,
  exactCustomModelRouteRecord,
  selectableCustomModelProtocols,
  shouldShowCustomModelProtocolPreferenceCard,
  shouldShowCustomModelProtocolSelector,
} from "@/components/features/settings/custom-provider-protocol-drafts"
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
  savePublicAiCustomModelProtocolPreferences,
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
  PublicAiCustomModelProtocolPreference,
  PublicAiCustomModelRequestProjection,
  PublicAiCustomProviderModelDefaults,
  PublicAiCustomProviderProtocol,
  PublicAiCustomModelTemplateDecision,
  PublicAiCustomModelTemplateDecisionBatchItem,
  PublicAiCustomModelTemplateSuggestionsRecord,
  PublicAiModelCapabilityOverrides,
  PublicAiModelRouteRecord,
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

// EMPTY_PROJECTION is the safe request projection used until the user declares exact mappings.
// EMPTY_PROJECTION 是用户声明精确映射前使用的安全请求投影。
const EMPTY_PROJECTION: PublicAiCustomModelRequestProjection = {
  reasoning: {},
  model_parameter_paths: {},
  defaults: {},
  overrides: {},
  delete: [],
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

// REASONING_EFFORT_LABEL_KEYS maps canonical effort values to localized labels.
// REASONING_EFFORT_LABEL_KEYS 将规范思考档位映射为本地化标签。
const REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
    medium: "settings.customModels.effortMedium",
    high: "settings.customModels.effortHigh",
    xhigh: "settings.customModels.effortExtraHigh",
    max: "settings.customModels.effortMax",
  } as const

// CustomProviderModelsDialog manages one merged model union and credential-scoped evidence.
// CustomProviderModelsDialog 管理一个合并模型并集与凭据作用域证据。

// ProtocolPreferenceCard edits one model's Auto/Explicit protocol choice using only Service-resolved
// data; the browser never resolves rules or inheritance (completion gate).
// ProtocolPreferenceCard 仅使用 Service 已解析数据编辑一个模型的 Auto/Explicit 协议选择；
// 浏览器绝不解析规则或继承（完成门）。
function ProtocolPreferenceCard(props: {
  enabledProtocols: PublicAiCustomProviderProtocol[]
  modelId: string
  onSaved: (preferences: { modelId: string; preference: PublicAiCustomModelProtocolPreference }[]) => void
  preference: PublicAiCustomModelProtocolPreference
  routeRecord?: PublicAiModelRouteRecord
  saving: boolean
}) {
  const { t } = useI18n()
  // selectableProtocols consumes backend-observed model variants and never reimplements Auto.
  // selectableProtocols 消费后端观测的模型变体，绝不重新实现 Auto。
  const selectableProtocols = React.useMemo(
    () => selectableCustomModelProtocols(props.enabledProtocols, props.routeRecord),
    [props.enabledProtocols, props.routeRecord],
  )
  // initialProtocol preserves a valid explicit preference or selects the first exact available
  // option for deliberate recovery from a stale explicit preference.
  // initialProtocol 保留有效显式偏好；显式偏好失效时选择首个精确可用项供用户主动恢复。
  const initialProtocol = props.preference.mode === "explicit"
    && selectableProtocols.includes(props.preference.protocol)
    ? props.preference.protocol
    : (
        selectableProtocols[0]
        ?? (props.preference.mode === "explicit" ? props.preference.protocol : undefined)
      )
  const [mode, setMode] = React.useState<"auto" | "explicit">(
    props.preference.mode === "explicit" ? "explicit" : "auto",
  )
  const [protocol, setProtocol] = React.useState<PublicAiCustomProviderProtocol | undefined>(initialProtocol)
  // multiProtocolOnly shows the selector only for providers with more than one enabled protocol.
  // multiProtocolOnly 仅在供应商启用多个协议时显示选择器。
  const multiProtocolOnly = shouldShowCustomModelProtocolSelector(props.enabledProtocols)
  // preferenceCardVisible keeps a recovery-only Auto action visible when a removed explicit
  // protocol has intentionally left the model blocked.
  // preferenceCardVisible 在已删除的显式协议刻意使模型保持阻塞时，保留仅用于恢复的
  // Auto 操作入口。
  const preferenceCardVisible = shouldShowCustomModelProtocolPreferenceCard(
    props.enabledProtocols,
    props.preference,
  )
  if (!preferenceCardVisible) {
    return null
  }
  // selectedPreference stays absent until the user selects one exact backend-observed protocol;
  // the browser must never invent a route merely to satisfy a controlled component.
  // selectedPreference 在用户选择精确的后端观测协议前保持缺失；
  // 浏览器绝不得仅为满足受控组件而伪造路由。
  const selectedPreference: PublicAiCustomModelProtocolPreference | null = mode === "auto"
    ? { mode: "auto" }
    : (protocol ? { mode: "explicit", protocol } : null)
  const selectionValid = selectedPreference !== null
    && (selectedPreference.mode === "auto"
      || selectableProtocols.includes(selectedPreference.protocol))
  const dirty = selectedPreference !== null
    && (selectedPreference.mode !== props.preference.mode
    || (selectedPreference.mode === "explicit"
      && props.preference.mode === "explicit"
      && selectedPreference.protocol !== props.preference.protocol))
  return (
    <div className="space-y-3 rounded-lg border p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-sm font-medium">{t("settings.customModels.protocolPreference")}</p>
          <p className="text-xs text-muted-foreground">{t("settings.customModels.protocolPreferenceHint")}</p>
        </div>
        <RadioGroup
          disabled={props.saving}
          onValueChange={(value) => setMode(value as "auto" | "explicit")}
          value={mode}
        >
          <div className="flex items-center gap-4">
            <label className="flex items-center gap-2 text-sm"><RadioGroupItem value="auto" />{t("settings.customModels.protocolAuto")}</label>
            {multiProtocolOnly ? (
              <label className="flex items-center gap-2 text-sm"><RadioGroupItem value="explicit" />{t("settings.customModels.protocolExplicit")}</label>
            ) : null}
          </div>
        </RadioGroup>
      </div>
      {mode === "explicit" && multiProtocolOnly ? (
        <Select
          disabled={props.saving}
          onValueChange={(value) => setProtocol(value as PublicAiCustomProviderProtocol)}
          value={protocol}
        >
          <SelectTrigger className="w-64"><SelectValue placeholder={t("settings.customModels.protocolSelect")} /></SelectTrigger>
          <SelectContent>
            {selectableProtocols.map((enabledProtocol) => (
              <SelectItem key={enabledProtocol} value={enabledProtocol}>
                {customProtocolProfileId(enabledProtocol)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : null}
      {props.routeRecord ? (
        <dl className="grid gap-2 rounded-md bg-muted/45 p-3 text-xs sm:grid-cols-2">
          <div className="min-w-0">
            <dt className="text-muted-foreground">{t("settings.models.routeEffectiveProtocol")}</dt>
            <dd className="truncate font-mono" title={props.routeRecord.protocolId}>
              {props.routeRecord.protocolId ?? "—"}
            </dd>
          </div>
          <div className="min-w-0">
            <dt className="text-muted-foreground">{t("settings.models.routeAvailableProtocols")}</dt>
            <dd
              className="truncate font-mono"
              title={props.routeRecord.availableProtocols.join(", ")}
            >
              {props.routeRecord.availableProtocols.join(", ") || "—"}
            </dd>
          </div>
          <div className="min-w-0">
            <dt className="text-muted-foreground">{t("settings.models.routeBindingSource")}</dt>
            <dd className="truncate font-mono" title={props.routeRecord.bindingSource}>
              {props.routeRecord.bindingSource ?? "—"}
            </dd>
          </div>
          <div className="min-w-0">
            <dt className="text-muted-foreground">{t("settings.models.routeBindingRule")}</dt>
            <dd className="truncate font-mono" title={props.routeRecord.bindingRuleId}>
              {props.routeRecord.bindingRuleId ?? t("settings.models.routeNoRule")}
            </dd>
          </div>
          <div className="min-w-0">
            <dt className="text-muted-foreground">{t("settings.models.routeManifestRevision")}</dt>
            <dd className="font-mono">{props.routeRecord.manifestRevision}</dd>
          </div>
        </dl>
      ) : (
        <p className="rounded-md border border-destructive/35 bg-destructive/5 p-3 text-xs text-destructive">
          {t("settings.models.routeUnresolved")}
        </p>
      )}
      {props.routeRecord && !props.routeRecord.executable ? (
        <p className="rounded-md border border-destructive/35 bg-destructive/5 p-3 text-xs text-destructive">
          <span className="font-medium">{t("settings.models.routeUnavailableReason")}: </span>
          {props.routeRecord.unavailableReason ?? t("settings.models.routeUnresolved")}
        </p>
      ) : null}
      <div className="flex justify-end">
        <Button
          disabled={props.saving || !dirty || !selectionValid}
          onClick={() => {
            if (selectedPreference) {
              props.onSaved([{ modelId: props.modelId, preference: selectedPreference }])
            }
          }}
          size="sm"
          type="button"
        >
          {t("settings.customModels.saveProtocolPreference")}
        </Button>
      </div>
    </div>
  )
}

export function CustomProviderModelsDialog(props: CustomProviderModelsDialogProps) {
  const { t } = useI18n()
  const [manualModels, setManualModels] = React.useState<PublicAiCustomModelDefinition[]>([])
  const [selectedModelId, setSelectedModelId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [filter, setFilter] = React.useState<CustomModelFilter>("all")
  const [advancedTextDrafts, setAdvancedTextDrafts] = React.useState<Record<string, AdvancedModelTextDraft>>({})
  const [dirtyManualModelIds, setDirtyManualModelIds] = React.useState<Set<string>>(new Set())
  // manualModelsDirty is derived from the single authoritative dirty-model identity set.
  // manualModelsDirty 从唯一权威的脏模型身份集合派生。
  const manualModelsDirty = dirtyManualModelIds.size > 0
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [removeConfirmationOpen, setRemoveConfirmationOpen] = React.useState(false)
  const [manualDialogOpen, setManualDialogOpen] = React.useState(false)
  const [mobileDetailOpen, setMobileDetailOpen] = React.useState(false)
  const [selectedConnectionId, setSelectedConnectionId] = React.useState<string | null>(null)
  const [discardConfirmationOpen, setDiscardConfirmationOpen] = React.useState(false)
  const [advancedSaveConfirmationOpen, setAdvancedSaveConfirmationOpen] = React.useState(false)
  const [templateReviewOpen, setTemplateReviewOpen] = React.useState(false)

  // protocolPreferences derives from the explicit snapshot records so the browser never parses
  // composite keys (completion gate: no second protocol-resolution implementation).
  // protocolPreferences 从显式快照记录派生，浏览器绝不解析组合键
  // （完成门：不允许第二份协议解析实现）。
  const protocolPreferences = React.useMemo(() => {
    if (!props.snapshot || !props.provider) {
      return new Map<string, PublicAiCustomModelProtocolPreference>()
    }
    return new Map(
      (props.snapshot.customModelProtocolPreferenceRecords ?? [])
        .filter((record) => record.providerId === props.provider?.id)
        .map((record) => [record.modelId, record.preference]),
    )
  }, [props.provider, props.snapshot])
  // enabledProtocols converts runtime profile identifiers back to the exact persisted custom
  // protocol contract, removes duplicate routes, and preserves fixed Auto priority.
  // enabledProtocols 将运行时配置档标识转换回精确持久化的自定义协议契约，去除重复路由，
  // 并保持固定 Auto 优先级。
  const enabledProtocols = React.useMemo(() => {
    if (!props.provider) {
      return []
    }
    return customProtocolsFromServiceEntries(props.provider.service_entries)
  }, [props.provider])

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
  // selectedRouteRecord selects only the exact backend-resolved provider/model/account identity;
  // missing evidence remains visibly unresolved instead of borrowing a sibling account route.
  // selectedRouteRecord 仅选择精确的后端已解析供应商/模型/账号身份；证据缺失时保持
  // 可见的未解析状态，而不会借用同级账号路由。
  const selectedRouteRecord = React.useMemo(() => {
    if (!props.provider || !selectedRow) {
      return undefined
    }
    return exactCustomModelRouteRecord(
      props.snapshot?.modelRouteRecords,
      props.provider.id,
      selectedRow.id,
      selectedAccount?.connection.id,
    )
  }, [props.provider, props.snapshot?.modelRouteRecords, selectedAccount?.connection.id, selectedRow])
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
    ? resolveDirectCustomProviderDefaults(
      props.snapshot.customProviderSystemModelDefaults,
      props.snapshot.customProviderModelDefaults?.[props.provider.id],
    )
    : null

  // selectedAdvancedDefinition exposes an inherited model as an immediately editable advanced draft.
  // selectedAdvancedDefinition 将继承模型直接呈现为可编辑高级草稿。
  const selectedAdvancedDefinition = selectedRow
    ? selectedRow.manualDefinition ?? newManualModel(selectedRow.id, selectedRow.displayName)
    : null
  // selectedAdvancedDraft preserves raw JSON edits while deriving untouched text from the visible definition.
  // selectedAdvancedDraft 保留原始 JSON 编辑，同时从可见定义派生未修改文本。
  const selectedAdvancedDraft = selectedAdvancedDefinition
    ? advancedTextDrafts[selectedAdvancedDefinition.id]
      ?? advancedModelTextDraftFromDefinition(selectedAdvancedDefinition)
    : { projectionText: JSON.stringify(EMPTY_PROJECTION, null, 2), parameterText: "[]" }

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

  // persistProtocolPreferences writes the complete per-model preference set for this provider.
  // persistProtocolPreferences 为该供应商写入完整逐模型偏好集合。
  const persistProtocolPreferences = React.useCallback(async (
    preferences: { modelId: string; preference: PublicAiCustomModelProtocolPreference }[],
  ) => {
    if (!props.provider) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      const snapshot = await savePublicAiCustomModelProtocolPreferences(
        props.provider.id,
        mergeCustomModelProtocolPreferences(protocolPreferences, preferences),
      )
      props.onSaved(snapshot)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [props, protocolPreferences])

  // updateManualDefinition materializes an inherited definition on its first direct edit.
  // updateManualDefinition 在首次直接编辑时实体化继承定义。
  const updateManualDefinition = React.useCallback((
    baseModel: PublicAiCustomModelDefinition,
    update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition,
  ) => {
    setManualModels((current) => {
      // existing is the only persisted or locally materialized definition for this model identity.
      // existing 是该模型身份唯一的持久化或本地实体化定义。
      const existing = current.find((model) => model.id === baseModel.id)
      // updated applies the edit to the exact current definition or its visible inherited draft.
      // updated 将编辑应用到精确当前定义或其可见继承草稿。
      const updated = update(existing ?? baseModel)
      return existing
        ? current.map((model) => model.id === baseModel.id ? updated : model)
        : [...current, updated]
    })
    setDirtyManualModelIds((current) => new Set(current).add(baseModel.id))
  }, [])

  // updateAdvancedTextDraft records raw JSON text without losing sibling-model edits.
  // updateAdvancedTextDraft 记录原始 JSON 文本且不会丢失其他模型的编辑。
  const updateAdvancedTextDraft = React.useCallback((
    model: PublicAiCustomModelDefinition,
    field: keyof AdvancedModelTextDraft,
    value: string,
  ) => {
    setManualModels((current) => current.some((candidate) => candidate.id === model.id)
      ? current
      : [...current, model])
    setAdvancedTextDrafts((current) => {
      const existing = current[model.id] ?? advancedModelTextDraftFromDefinition(model)
      return { ...current, [model.id]: { ...existing, [field]: value } }
    })
    setDirtyManualModelIds((current) => new Set(current).add(model.id))
  }, [])

  // discardInheritedAdvancedDraft removes one unsaved materialized draft without calling the deletion API.
  // discardInheritedAdvancedDraft 移除一个尚未保存的实体化草稿，且不调用删除接口。
  const discardInheritedAdvancedDraft = React.useCallback((modelId: string) => {
    setManualModels((current) => current.filter((model) => model.id !== modelId))
    setAdvancedTextDrafts((current) => {
      // next retains every sibling model draft while removing the exact restored identity.
      // next 保留全部同级模型草稿，同时移除精确还原的模型身份。
      const next = { ...current }
      delete next[modelId]
      return next
    })
    setDirtyManualModelIds((current) => {
      // next is the complete remaining dirty-model identity set.
      // next 是完整的剩余脏模型身份集合。
      const next = new Set(current)
      next.delete(modelId)
      return next
    })
  }, [])

  // stageDiscoveredAdvancedRestore removes one discovered-model overlay locally until the reviewed batch save.
  // stageDiscoveredAdvancedRestore 在经审查的批量保存前，本地移除一个已发现模型的高级覆盖。
  const stageDiscoveredAdvancedRestore = React.useCallback((modelId: string) => {
    setManualModels((current) => current.filter((model) => model.id !== modelId))
    setAdvancedTextDrafts((current) => {
      // next removes stale JSON text that no longer belongs to the restored inherited model.
      // next 移除不再属于已还原继承模型的陈旧 JSON 文本。
      const next = { ...current }
      delete next[modelId]
      return next
    })
    setDirtyManualModelIds((current) => new Set(current).add(modelId))
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
        <Tabs className="min-h-0 flex-1 gap-0" defaultValue="models">
          <div className="shrink-0 border-b px-4 py-2">
            <TabsList variant="line">
              <TabsTrigger className="px-4" value="models">{t("settings.customModels.pageModels")}</TabsTrigger>
              <TabsTrigger className="px-4" value="provider-settings">{t("settings.customModels.pageProviderSettings")}</TabsTrigger>
            </TabsList>
          </div>
          <TabsContent className="m-0 min-h-0 overflow-hidden" value="models">
            <div className="grid h-full min-h-0 md:grid-cols-[320px_minmax(0,1fr)]">
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
                      disabled={saving}
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
                  <ModelDetailHeader disabled={saving} onEnabledChange={(enabled) => void setModelEnabled(selectedRow, enabled)} row={selectedRow} />
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
                      <ProtocolPreferenceCard
                        enabledProtocols={enabledProtocols}
                        key={`${selectedRow.id}:${selectedAccount?.connection.id ?? "no-account"}`}
                        modelId={selectedRow.id}
                        onSaved={persistProtocolPreferences}
                        preference={protocolPreferences.get(selectedRow.id) ?? { mode: "auto" }}
                        routeRecord={selectedRouteRecord}
                        saving={saving}
                      />
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
                      {selectedAdvancedDefinition ? <AdvancedModelEditor
                        customized={Boolean(selectedRow.manualDefinition) || dirtyManualModelIds.has(selectedRow.id)}
                        dirty={dirtyManualModelIds.has(selectedRow.id)}
                        manualDefinition={selectedAdvancedDefinition}
                        onRestore={() => {
                          if (selectedRow.manualDefinition) {
                            if (selectedRow.accounts.some(accountActuallyDiscoveredModel)) {
                              stageDiscoveredAdvancedRestore(selectedRow.id)
                            } else {
                              setRemoveConfirmationOpen(true)
                            }
                          } else {
                            discardInheritedAdvancedDraft(selectedRow.id)
                          }
                        }}
                        onSave={() => setAdvancedSaveConfirmationOpen(true)}
                        onUpdate={(update) => updateManualDefinition(selectedAdvancedDefinition, update)}
                        parameterText={selectedAdvancedDraft.parameterText}
                        projectionText={selectedAdvancedDraft.projectionText}
                        setParameterText={(value) => updateAdvancedTextDraft(selectedAdvancedDefinition, "parameterText", value)}
                        setProjectionText={(value) => updateAdvancedTextDraft(selectedAdvancedDefinition, "projectionText", value)}
                        saving={saving}
                      /> : null}
                    </TabsContent>
                  </Tabs>
                </div>
              </ScrollArea>
            ) : (
              <div className="grid h-full place-items-center p-8 text-center text-sm text-muted-foreground">{t("settings.customModels.selectHint")}</div>
            )}
          </section>
            </div>
          </TabsContent>
          <TabsContent className="m-0 min-h-0 overflow-hidden" value="provider-settings">
            {props.provider && props.snapshot && providerDefaults ? (
              <ScrollArea className="h-full">
                <div className="mx-auto grid w-full max-w-[1120px] items-start gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_minmax(420px,0.9fr)]">
                  <DiscoverySummary
                    onRefresh={() => void synchronizeAll()}
                    onRefreshConnection={(connectionId) => void synchronizeConnection(connectionId)}
                    onReviewTemplates={() => setTemplateReviewOpen(true)}
                    providerId={props.provider.id}
                    snapshot={props.snapshot}
                  />
                  <CustomProviderModelDefaultsCard
                    customized={Boolean(props.snapshot.customProviderModelDefaults?.[props.provider.id])}
                    defaults={providerDefaults}
                    onSaved={acceptSnapshot}
                    providerId={props.provider.id}
                  />
                </div>
              </ScrollArea>
            ) : null}
          </TabsContent>
        </Tabs>
        {error ? <p className="whitespace-pre-wrap border-t bg-destructive/5 px-5 py-3 text-sm text-destructive">{error}</p> : null}
      </DialogContent>
      {providerDefaults ? <ManualModelDialog
        defaults={providerDefaults}
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
      /> : null}
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

// TemplateReviewDialog groups deterministic matches and unmatched offline capability records.
// TemplateReviewDialog 对确定性命中与无命中的离线能力记录进行分组。
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
  const exactRecords = undecidedRecords.filter((record) => record.suggestions.length > 0)
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
        <div className="grid grid-cols-2 gap-3 border-b p-4">
          <TemplateReviewCount label={t("settings.customModels.reviewExact")} value={exactRecords.length} />
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
      output_resources: [],
    },
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
function ModelDetailHeader(props: { disabled: boolean; onEnabledChange: (enabled: boolean) => void; row: CustomProviderModelRow }) {
  const { t } = useI18n()
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <h3 className="truncate text-lg font-semibold">{props.row.displayName}</h3>
        <p className="truncate font-mono text-xs text-muted-foreground">{props.row.id}</p>
      </div>
      <label className="flex items-center gap-2 text-sm"><span>{t("settings.customModels.enabled")}</span><Switch checked={props.row.enabled} disabled={props.disabled} onCheckedChange={props.onEnabledChange} /></label>
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
  const capabilities = props.account?.effectiveModel?.capabilities
    ?? props.account?.supportModel?.capabilities
    ?? props.row.capabilityModel?.capabilities
    ?? props.row.effectiveModel?.capabilities
  const context = capabilities?.context_limit ?? props.defaults?.context_limit
  const output = capabilities?.output_limit ?? props.defaults?.output_limit
  const inputHasText = resolvedInputTextSupport(props.row, props.account)
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <CapabilityFact label={t("settings.customModels.contextLimit")} source={capabilitySource(props.row, props.account, "context_limit", t)} value={context?.toLocaleString() ?? "—"} />
      <CapabilityFact label={t("settings.customModels.outputLimit")} source={capabilitySource(props.row, props.account, "output_limit", t)} value={output?.toLocaleString() ?? "—"} />
      <CapabilityFact label={t("settings.customModels.input")} source={capabilitySource(props.row, props.account, "input_resources", t)} value={resourceSummary(capabilities?.input_resources ?? [], inputHasText, t)} />
      <CapabilityFact label={t("settings.customModels.output")} source={t("settings.customModels.defaultsSystemSource")} value={t("settings.models.modality.text")} />
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
  const [error, setError] = React.useState<string | null>(null)

  // saveAvailability immediately persists one direct account-model membership choice or restoration.
  // saveAvailability 立即持久化一个直接账号模型成员关系选择或还原操作。
  const saveAvailability = React.useCallback(async (
    connectionId: string,
    enabled: boolean | undefined,
  ) => {
    setSavingConnectionId(connectionId)
    setError(null)
    try {
      props.onSaved(await savePublicAiCustomModelConnectionAvailability({
        connectionId,
        modelId: props.row.id,
        enabled,
      }))
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
              <div className="flex items-center gap-2">
                {account.availabilityOverride !== undefined ? (
                  <Button
                    aria-label={t("settings.modelCapabilities.restore")}
                    disabled={savingConnectionId === account.connection.id}
                    onClick={() => void saveAvailability(account.connection.id, undefined)}
                    size="icon"
                    title={t("settings.modelCapabilities.restore")}
                    type="button"
                    variant="ghost"
                  >
                    <RotateCcw className="size-4" />
                  </Button>
                ) : null}
                {savingConnectionId === account.connection.id ? (
                  <LoaderCircle className="size-4 animate-spin text-muted-foreground" />
                ) : null}
                <Switch
                  aria-label={`${account.connection.display_name} · ${t("settings.customModels.accountAvailable")}`}
                  checked={props.row.enabled && accountIncludesModel(account)}
                  disabled={!props.row.enabled || savingConnectionId !== null}
                  onCheckedChange={(enabled) => void saveAvailability(account.connection.id, enabled)}
                />
              </div>
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
    </div>
  )
}

// CapabilityOverridesEditor displays effective values while persisting only touched model fields.
// CapabilityOverridesEditor 显示有效值，但只持久化用户触碰过的模型字段。
function CapabilityOverridesEditor(props: { onSaved: (snapshot: PublicAiSettingsSnapshot) => void; providerId: string; row: CustomProviderModelRow }) {
  const { t } = useI18n()
  // draft is the complete visible model-level setting set, never a second enablement gate.
  // draft 是完整可见的模型级设置集合，不再是二次启用开关。
  const [draft, setDraft] = React.useState<PublicAiModelCapabilityOverrides>(() => directCustomModelSettingsDraft(props.row))
  // baselineDraft is the authoritative value against which local semantic changes are measured.
  // baselineDraft 是衡量本地语义变更所依据的权威值。
  const [baselineDraft, setBaselineDraft] = React.useState<PublicAiModelCapabilityOverrides>(() => directCustomModelSettingsDraft(props.row))
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [edited, setEdited] = React.useState(false)
  // touchedFields preserves provider inheritance when one visible field is edited.
  // touchedFields 在只编辑一个可见字段时保留供应商继承关系。
  const [touchedFields, setTouchedFields] = React.useState<Set<string>>(new Set())
  // dirty protects a real in-progress edit from unrelated runtime snapshot refreshes.
  // dirty 保护真实进行中的编辑，避免被无关运行时快照刷新覆盖。
  const dirty = !directProviderModelSettingsEqual(draft, baselineDraft)
  // draftModelKey uniquely identifies the provider-model editor identity.
  // draftModelKey 唯一标识供应商模型编辑器身份。
  const draftModelKey = `${props.providerId}\u0000${props.row.id}`
  // initializedModelKey tracks which identity currently owns the local draft.
  // initializedModelKey 跟踪当前本地草稿所属的身份。
  const initializedModelKey = React.useRef(draftModelKey)
  React.useEffect(() => {
    // modelChanged requires a full reset even when the previous model had unfinished edits.
    // modelChanged 即使前一模型存在未完成编辑，也要求完整重置。
    const modelChanged = initializedModelKey.current !== draftModelKey
    if (!modelChanged && (dirty || edited)) {
      return
    }
    // nextDraft becomes both the visible and comparison state for this authoritative row.
    // nextDraft 同时成为该权威模型行的可见状态与比较状态。
    const nextDraft = directCustomModelSettingsDraft(props.row)
    initializedModelKey.current = draftModelKey
    setDraft(nextDraft)
    setBaselineDraft(nextDraft)
    setError(null)
    setEdited(false)
    setTouchedFields(new Set())
  }, [dirty, draftModelKey, edited, props.row])

  // updateDraft changes visible values; semantic comparison derives whether a real edit exists.
  // updateDraft 修改可见值；是否存在真实编辑由语义比较推导。
  const updateDraft = React.useCallback((
    fields: string | readonly string[],
    update: (current: PublicAiModelCapabilityOverrides) => PublicAiModelCapabilityOverrides,
  ) => {
    setDraft(update)
    setEdited(true)
    setTouchedFields((current) => {
      const next = new Set(current)
      for (const field of typeof fields === "string" ? [fields] : fields) {
        next.add(field)
      }
      return next
    })
    setError(null)
  }, [])

  // settingsToPersist converts the complete visible draft into the sparse model patch accepted by the service.
  // settingsToPersist 将完整可见草稿转换为服务端接受的稀疏模型补丁。
  const settingsToPersist = React.useMemo(
    () => buildCapabilityOverridePatch(draft, props.row.capabilityOverrides, touchedFields),
    [draft, props.row.capabilityOverrides, touchedFields],
  )

  // persist writes the visible model layer or removes it to restore provider inheritance.
  // persist 写入可见模型层，或移除该层以恢复供应商继承。
  const persist = React.useCallback(async (restore: boolean) => {
    if (!restore && (!Number.isSafeInteger(draft.context_limit) || (draft.context_limit ?? 0) <= 0)) {
      setError(t("settings.modelCapabilities.contextInvalid"))
      return
    }
    if (!restore && (!Number.isSafeInteger(draft.output_limit) || (draft.output_limit ?? 0) <= 0 || (draft.output_limit ?? 0) > (draft.context_limit ?? 0))) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    if (!restore && draft.reasoning !== "unsupported" && !(draft.reasoning_efforts ?? []).some((effort) => effort !== "none")) {
      setError(t("settings.customModels.defaultsReasoningInvalid"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiModelCapabilityOverrides({
        providerId: props.providerId,
        modelId: props.row.id,
        overrides: restore ? undefined : settingsToPersist,
      }))
      setBaselineDraft(draft)
      setEdited(false)
      setTouchedFields(new Set())
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [draft, props, settingsToPersist, t])

  // restore discards an unsaved draft locally or removes an existing persisted model layer.
  // restore 在本地丢弃未保存草稿，或移除已持久化模型层。
  const restore = React.useCallback(() => {
    if (props.row.capabilityOverrides) {
      void persist(true)
      return
    }
    // nextDraft discards only the unsaved local delta when no persisted model layer exists.
    // nextDraft 在不存在已持久化模型层时，仅丢弃未保存的本地差异。
    const nextDraft = directCustomModelSettingsDraft(props.row)
    setDraft(nextDraft)
    setBaselineDraft(nextDraft)
    setEdited(false)
    setTouchedFields(new Set())
    setError(null)
  }, [persist, props.row])

  // reasoningEnabled projects the paired support and control contract into one clear switch.
  // reasoningEnabled 将配对的支持与控制契约投影为一个清晰开关。
  const reasoningEnabled = draft.reasoning !== "unsupported" && draft.reasoning_control !== "fixed_off"
  return (
    <div className="space-y-4 rounded-lg border p-4">
      <div className="flex items-center justify-between gap-3">
        <p className="text-sm text-muted-foreground">{t("settings.modelCapabilities.overlayHint")}</p>
        <Badge variant={props.row.capabilityOverrides ? "default" : "outline"}>{t(props.row.capabilityOverrides ? "settings.modelCapabilities.modelSource" : "settings.modelCapabilities.inheritedSource")}</Badge>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-2 text-sm font-medium">{t("settings.customModels.contextLimit")}<Input inputMode="numeric" onChange={(event) => updateDraft("context_limit", (current) => ({ ...current, context_limit: Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10) }))} value={draft.context_limit ?? ""} /></label>
        <label className="grid gap-2 text-sm font-medium">{t("settings.customModels.outputLimit")}<Input inputMode="numeric" onChange={(event) => updateDraft("output_limit", (current) => ({ ...current, output_limit: Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10) }))} value={draft.output_limit ?? ""} /></label>
      </div>
      <ContextWindowPolicyEditor
        onChange={(value) => updateDraft("context_window_policy", (current) => ({ ...current, context_window_policy: value }))}
        value={draft.context_window_policy ?? undefined}
      />
      <fieldset className="rounded-lg border p-3">
        <legend className="px-1 text-sm font-medium">{t("settings.customModels.input")}</legend>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          {RESOURCE_KINDS.map((resource) => <label className="flex items-center gap-2 text-sm" key={resource}><Checkbox checked={(draft.input_resources ?? []).includes(resource)} onCheckedChange={(checked) => updateDraft("input_resources", (current) => ({ ...current, input_resources: checked ? [...(current.input_resources ?? []), resource] : (current.input_resources ?? []).filter((item) => item !== resource) }))} />{t(RESOURCE_LABEL_KEYS[resource])}</label>)}
        </div>
      </fieldset>
      <div className="rounded-lg border bg-muted/30 p-3">
        <p className="text-sm font-medium">{t("settings.customModels.output")}: {t("settings.models.modality.text")}</p>
        <p className="mt-1 text-xs text-muted-foreground">{t("settings.customModels.fixedTextOutputDescription")}</p>
      </div>
      <div className="space-y-3 rounded-lg border p-3">
        <label className="flex items-center justify-between gap-3 text-sm font-medium"><span>{t("settings.customModels.reasoning")}</span><Switch checked={reasoningEnabled} onCheckedChange={(checked) => updateDraft(["reasoning", "reasoning_control", "reasoning_efforts"], (current) => ({ ...current, reasoning: checked ? "native" : "unsupported", reasoning_control: checked ? "effort_selection" : "fixed_off", reasoning_efforts: checked ? [...OPENAI_DIRECT_REASONING_EFFORTS] : ["none"] }))} /></label>
        {reasoningEnabled ? <div className="flex flex-wrap gap-2">{OPENAI_DIRECT_REASONING_EFFORTS.map((effort) => <label className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm" key={effort}><Checkbox checked={(draft.reasoning_efforts ?? []).includes(effort)} disabled={effort !== "none" && (draft.reasoning_efforts ?? []).includes(effort) && (draft.reasoning_efforts ?? []).filter((item) => item !== "none").length === 1} onCheckedChange={(checked) => updateDraft("reasoning_efforts", (current) => ({ ...current, reasoning_efforts: checked ? [...(current.reasoning_efforts ?? []), effort] : (current.reasoning_efforts ?? []).filter((item) => item !== effort) }))} />{t(REASONING_EFFORT_LABEL_KEYS[effort])}</label>)}</div> : null}
      </div>
      <ReasoningProjectionEditor
        efforts={draft.reasoning_efforts?.length ? draft.reasoning_efforts : ["none"]}
        onChange={(request_projection) => updateDraft("request_projection", (current) => ({ ...current, request_projection }))}
        projection={draft.request_projection ?? EMPTY_PROJECTION}
      />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <div className="flex justify-end gap-2"><Button disabled={saving || (!props.row.capabilityOverrides && !dirty && !edited)} onClick={restore} type="button" variant="outline">{t("settings.modelCapabilities.restore")}</Button><Button disabled={saving || (!dirty && !edited)} onClick={() => void persist(false)} type="button">{saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}{t("settings.customModels.saveCapabilityOverrides")}</Button></div>
    </div>
  )
}

// AdvancedModelEditor edits manual identity metadata and protocol-specific exact declarations.
// AdvancedModelEditor 编辑手工身份元数据与协议专属精确声明。
function AdvancedModelEditor(props: {
  customized: boolean
  dirty: boolean
  manualDefinition: PublicAiCustomModelDefinition
  onRestore: () => void
  onSave: () => void
  onUpdate: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void
  parameterText: string
  projectionText: string
  setParameterText: (value: string) => void
  setProjectionText: (value: string) => void
  saving: boolean
}) {
  const { t } = useI18n()
  // model is always the visible inherited or explicit definition and is editable immediately.
  // model 始终是可见的继承或显式定义，并且可以立即编辑。
  const model = props.manualDefinition
  return (
    <div className="space-y-4">
      <label className="grid gap-2 text-sm font-medium">{t("settings.customModels.displayName")}<Input onChange={(event) => props.onUpdate((current) => ({ ...current, display_name: event.target.value }))} value={model.display_name} /></label>
      <Accordion collapsible type="single"><AccordionItem value="json"><AccordionTrigger>{t("settings.customModels.advanced")}</AccordionTrigger><AccordionContent className="space-y-4"><label className="grid gap-2 text-sm font-medium">{t("settings.customModels.parameters")}<Textarea className="min-h-32 font-mono text-xs" onChange={(event) => props.setParameterText(event.target.value)} spellCheck={false} value={props.parameterText} /></label><label className="grid gap-2 text-sm font-medium">{t("settings.customModels.projection")}<Textarea className="min-h-52 font-mono text-xs" onChange={(event) => props.setProjectionText(event.target.value)} spellCheck={false} value={props.projectionText} /></label></AccordionContent></AccordionItem></Accordion>
      <div className="flex flex-wrap justify-end gap-2">{props.customized ? <Button disabled={props.saving} onClick={props.onRestore} type="button" variant="outline"><Trash2 className="mr-2 size-4" />{t("settings.modelCapabilities.restore")}</Button> : null}<Button disabled={props.saving || !props.dirty} onClick={props.onSave} type="button">{t("settings.customModels.saveAdvanced")}</Button></div>
    </div>
  )
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
    const decision = selected ? { decision: "applied" as const, selection: { template_key: selected.template_key, template_revision: selected.template_revision, context_limit: selected.context_limit, supports_text_input: selected.supports_text_input, supports_text_output: selected.supports_text_output, input_resources: selected.input_resources, output_resources: [] } } : firstSuggestion ? { decision: "rejected" as const, template_key: firstSuggestion.template_key, template_revision: firstSuggestion.template_revision } : undefined
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
                    <span className="block text-xs text-muted-foreground">
                      {suggestion.context_limit.toLocaleString()} Token · {t(`settings.customModels.templateMatch.${suggestion.match_kind}`)}
                    </span>
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
  return { id: modelId, display_name: displayName, enabled: true, capabilities: { operations: ["conversation_respond"], input_resources: [], output_resources: [], evidence_revision: "user.manual-model.v2", reasoning: "unknown", reasoning_control: "unknown", reasoning_efforts: [], prompt_cache: { provider_default_behavior: "Follow provider defaults without host-injected cache controls.", supports_managed_explicit: false, supported_ttl_options: [] }, model_parameters: [] }, request_projection: structuredClone(EMPTY_PROJECTION) }
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
    ?? account?.supportModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  if (evidence) {
    if (field === "context_limit" && evidence.context_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "output_limit" && evidence.output_limit_declared) return t("settings.customModels.sourceAccount")
    if (field === "input_resources" && evidence.input_modalities_declared) return t("settings.customModels.sourceAccount")
  }
  if (row.templateDecision?.decision === "applied" && field !== "output_limit") return t("settings.customModels.sourceTemplate")
  return t("settings.customModels.sourceProviderDefault")
}

// resolvedInputTextSupport resolves input text support without treating explicit false as missing.
// resolvedInputTextSupport 解析输入文本支持且不会把显式 false 当作缺失值。
function resolvedInputTextSupport(
  row: CustomProviderModelRow,
  account: CustomProviderModelAccountState | undefined,
): boolean {
  const evidence = account?.discoveredModel?.capability_evidence
    ?? account?.supportModel?.capability_evidence
    ?? account?.effectiveModel?.capability_evidence
  if (evidence?.input_modalities_declared) {
    return evidence.supports_text_input === true
  }
  if (row.templateDecision?.decision === "applied") {
    return row.templateDecision.selection.supports_text_input
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

// cloneCustomModel creates an isolated editable copy of one persisted model definition.
// cloneCustomModel 创建一个持久化模型定义的隔离可编辑副本。
function cloneCustomModel(model: PublicAiCustomModelDefinition): PublicAiCustomModelDefinition { return structuredClone(model) }

// errorMessage normalizes rejected service and JSON errors for display.
// errorMessage 规范化服务拒绝与 JSON 错误以供显示。
function errorMessage(reason: unknown): string { return reason instanceof Error ? reason.message : String(reason) }
