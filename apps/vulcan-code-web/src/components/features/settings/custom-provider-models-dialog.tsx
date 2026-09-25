import * as React from "react"
import { ArrowLeft, Cloud, FilePlus2, Plus, RefreshCw, Search, Sparkles, Users } from "lucide-react"

import { CustomModelTemplateCard } from "@/components/features/settings/custom-model-template-card"
import { CustomProviderChangeSummaryDialog } from "@/components/features/settings/custom-provider-change-summary-dialog"
import {
  advancedModelTextDraftFromDefinition,
  applyAdvancedModelTextDrafts,
  type AdvancedModelTextDraft,
} from "@/components/features/settings/custom-provider-model-drafts"
import { CustomProviderModelDefaultsCard } from "@/components/features/settings/custom-provider-model-defaults-card"
import { CustomProviderMutationContext } from "./custom-provider-mutation-context"
import { resolveDirectCustomProviderDefaults } from "@/components/features/settings/direct-provider-model-settings"
import { accountActuallyDiscoveredModel, accountExposesModel, buildCustomProviderModelRows, mergeCustomModelProtocolPreferences, shouldReplaceManualModelDrafts, type CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import {
  customProtocolProfileId,
  customProtocolsFromServiceEntries,
  exactCustomModelRouteRecord,
  selectableCustomModelProtocols,
  shouldShowCustomModelProtocolPreferenceCard,
  shouldShowCustomModelProtocolSelector,
} from "@/components/features/settings/custom-provider-protocol-drafts"
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
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
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import { loadPublicAiSettings, savePublicAiCustomModelProtocolPreferences, savePublicAiCustomModels, syncPublicAiConnection } from "@/service/settings-actions-service"
import type { PublicAiCustomModelDefinition, PublicAiCustomModelProtocolPreference, PublicAiCustomProviderProtocol, PublicAiCustomModelTemplateDecisionBatchItem, PublicAiModelRouteRecord, PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types"
import { formatDateTime, errorMessage } from "@/lib/display-format"
import { EMPTY_PROJECTION, DISCOVERY_STATE_LABEL_KEYS } from "./custom-provider-models-dialog/model-dialog-constants"
import { TemplateReviewDialog } from "./custom-provider-models-dialog/template-review-dialog"
import { CapabilityAccountSelector, ResolvedCapabilitySummary, AccountAvailabilityEditor, CapabilityOverridesEditor, AdvancedModelEditor } from "./custom-provider-models-dialog/model-capability-editors"
import { ManualModelDialog, newManualModel } from "./custom-provider-models-dialog/manual-model-dialog"

// CustomProviderModelsDialogProps identifies the provider and immutable runtime snapshot being edited.
// CustomProviderModelsDialogProps 标识被编辑供应商及其不可变运行时快照。
interface CustomProviderModelsDialogProps {
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  provider: PublicAiProviderDescriptor | null
  snapshot: PublicAiSettingsSnapshot | null
}

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

// Parameters: props identifies the provider and controlled dialog callbacks; returns the coordinated model catalog, capability and account editors.
// 参数：props 标识供应商与受控弹窗回调；返回协同的模型目录、能力及账号编辑器。
export function CustomProviderModelsDialog(props: CustomProviderModelsDialogProps) {
  const { t } = useI18n()
  const [manualModels, setManualModels] = React.useState<PublicAiCustomModelDefinition[]>([])
  const [selectedModelId, setSelectedModelId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [advancedTextDrafts, setAdvancedTextDrafts] = React.useState<Record<string, AdvancedModelTextDraft>>({})
  const [dirtyManualModelIds, setDirtyManualModelIds] = React.useState<Set<string>>(new Set())
  // manualModelsDirty is derived from the single authoritative dirty-model identity set.
  // manualModelsDirty 从唯一权威的脏模型身份集合派生。
  const manualModelsDirty = dirtyManualModelIds.size > 0
  const [saving, setSaving] = React.useState(false)
  // savingRef serializes provider-level mutations before React updates the controls.
  // savingRef 在 React 更新控件前串行化供应商级变更。
  const savingRef = React.useRef(false)
  // mutationBoundary prevents nested account, capability and defaults editors from racing one another.
  // mutationBoundary 防止嵌套账号、能力和默认值编辑器彼此竞态。
  const mutationBoundary = React.useMemo(() => ({
    // acquire freezes all sibling controls before any asynchronous mutation begins.
    // acquire 在异步变更开始前冻结全部同级控件。
    acquire: () => {
      if (savingRef.current) return false
      savingRef.current = true
      setSaving(true)
      return true
    },
    // release restores navigation only after the owning operation has settled.
    // release 仅在所属操作结束后恢复导航。
    release: () => { savingRef.current = false; setSaving(false) },
  }), [])
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
      return !normalizedQuery
        || row.id.toLocaleLowerCase().includes(normalizedQuery)
        || row.displayName.toLocaleLowerCase().includes(normalizedQuery)
    })
  }, [query, rows])

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
    if (!props.provider || savingRef.current) return null
    savingRef.current = true
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
      savingRef.current = false
      setSaving(false)
    }
  }, [acceptSnapshot, advancedTextDrafts, manualModels, props.provider, t])

  // persistProtocolPreferences writes the complete per-model preference set for this provider.
  // persistProtocolPreferences 为该供应商写入完整逐模型偏好集合。
  const persistProtocolPreferences = React.useCallback(async (
    preferences: { modelId: string; preference: PublicAiCustomModelProtocolPreference }[],
  ) => {
    if (!props.provider || savingRef.current) {
      return
    }
    savingRef.current = true
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
      savingRef.current = false
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
    if (!props.provider || !props.snapshot || savingRef.current) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    // failures retains every account-scoped failure instead of stopping after the first account.
    // failures 保留每个账号作用域失败，而不是在首个账号失败后停止。
    const failures: string[] = []
    // connections is the immutable provider-scoped work queue captured before synchronization starts.
    // connections 是同步开始前捕获的不可变供应商作用域工作队列。
    const connections = props.snapshot.connections.filter((item) => item.provider_id === props.provider?.id && item.enabled)
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
    savingRef.current = false
    setSaving(false)
  }, [props])

  // synchronizeConnection refreshes one exact credential without discarding local advanced edits.
  // synchronizeConnection 刷新一个精确凭据且不会丢弃本地高级编辑。
  const synchronizeConnection = React.useCallback(async (connectionId: string) => {
    if (savingRef.current || !props.snapshot?.connections.some((connection) => connection.id === connectionId && connection.enabled)) return
    savingRef.current = true
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
      savingRef.current = false
      setSaving(false)
    }
  }, [props])

  // requestDialogOpenChange protects local advanced edits from an accidental close.
  // requestDialogOpenChange 防止本地高级编辑因误关闭而丢失。
  const requestDialogOpenChange = React.useCallback((open: boolean) => {
    if (savingRef.current) return
    if (!open && manualModelsDirty) {
      setDiscardConfirmationOpen(true)
      return
    }
    props.onOpenChange(open)
  }, [manualModelsDirty, props])

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
    <CustomProviderMutationContext.Provider value={mutationBoundary}>
    <Dialog onOpenChange={requestDialogOpenChange} open={props.open}>
      <DialogContent className="flex h-[min(860px,94vh)] w-[min(1180px,96vw)] max-w-[min(1180px,96vw)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(1180px,96vw)]">
        <DialogHeader className="border-b px-5 py-4">
          <DialogTitle>{t("settings.customModels.title").replace("{provider}", props.provider?.display_name ?? "")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.description")}</DialogDescription>
        </DialogHeader>
        <fieldset disabled={saving} className="flex min-h-0 min-w-0 flex-1 flex-col">
        <Tabs className="min-h-0 flex-1 gap-0" defaultValue="models">
          <div className="shrink-0 border-b px-4 py-2">
            <TabsList variant="line">
              <TabsTrigger className="px-4" value="models">{t("settings.customModels.pageModels")}</TabsTrigger>
              <TabsTrigger className="px-4" value="provider-settings">{t("settings.customModels.pageProviderSettings")}</TabsTrigger>
            </TabsList>
          </div>
          <TabsContent className="m-0 min-h-0 overflow-hidden data-[state=inactive]:hidden" forceMount value="models">
            <div className="grid h-full min-h-0 md:grid-cols-[320px_minmax(0,1fr)]">
          <section className={`${mobileDetailOpen ? "max-md:hidden" : ""} flex min-h-0 flex-col border-r`}>
            <div className="space-y-3 border-b p-3">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input className="pl-9" onChange={(event) => setQuery(event.target.value)} placeholder={t("settings.customModels.search")} value={query} />
              </div>
              <div className="flex justify-end gap-2">
                <Tooltip delayDuration={500}>
                  <TooltipTrigger asChild>
                    <Button aria-label={t("settings.customModels.addManual")} onClick={() => setManualDialogOpen(true)} size="icon" type="button">
                      <Plus className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{t("settings.customModels.addManual")}</TooltipContent>
                </Tooltip>
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
                      <span className="block whitespace-normal break-words text-sm font-medium">{row.displayName}</span>
                      <span className="block break-all font-mono text-xs text-muted-foreground">{row.id}</span>
                      <span className="mt-1 flex flex-wrap gap-1">
                        {row.accounts.some(accountActuallyDiscoveredModel) ? <Badge variant="outline"><Cloud className="mr-1 size-3" />{t("settings.customModels.sourceApi")}</Badge> : null}
                        {row.manualDefinition ? <Badge variant="outline"><FilePlus2 className="mr-1 size-3" />{t("settings.customModels.sourceManual")}</Badge> : null}
                        <Badge variant="secondary"><Users className="mr-1 size-3" />{row.accounts.filter(accountExposesModel).length}</Badge>
                      </span>
                    </button>
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
                  <ModelDetailHeader row={selectedRow} />
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
                    <TabsContent className="data-[state=inactive]:hidden" forceMount value="capabilities">
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
          <TabsContent className="m-0 min-h-0 overflow-hidden data-[state=inactive]:hidden" forceMount value="provider-settings">
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
                    key={props.provider.id}
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
        </fieldset>
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
      <AlertDialog onOpenChange={(open) => { if (!savingRef.current) setRemoveConfirmationOpen(open) }} open={removeConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.remove")}</AlertDialogTitle>
            <AlertDialogDescription>{t("settings.customModels.removeConfirm").replace("{name}", selectedRow?.displayName ?? "")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={saving}>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={saving} onClick={(event) => { event.preventDefault(); void removeManualDefinition() }} variant="destructive">{t("settings.customModels.remove")}</AlertDialogAction>
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
    </CustomProviderMutationContext.Provider>
  )
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
  const { t, locale } = useI18n()
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
          <p className="mt-1 text-xs text-muted-foreground">{t("settings.customModels.lastSynchronized").replace("{time}", formatObservationTime(latestObservation, t("settings.customModels.notSynchronized"), locale))}</p>
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
                          {" · "}{formatObservationTime(status?.observed_at_ms ?? 0, t("settings.customModels.notSynchronized"), locale)}
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

// ModelDetailHeader renders immutable model identity; availability is owned by the model list.
// ModelDetailHeader 渲染不可变模型身份；可用状态由模型列表管理。
function ModelDetailHeader(props: { row: CustomProviderModelRow }) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0">
        <h3 className="truncate text-lg font-semibold">{props.row.displayName}</h3>
        <p className="truncate font-mono text-xs text-muted-foreground">{props.row.id}</p>
      </div>
    </div>
  )
}

// formatObservationTime renders process-local discovery time without inventing a timestamp.
// formatObservationTime 渲染进程内发现时间且不会虚构时间戳。
function formatObservationTime(observedAtMs: number, fallback: string, locale: string): string {
  return observedAtMs > 0 ? formatDateTime(observedAtMs, locale) ?? fallback : fallback
}

// cloneCustomModel creates an isolated editable copy of one persisted model definition.
// cloneCustomModel 创建一个持久化模型定义的隔离可编辑副本。
function cloneCustomModel(model: PublicAiCustomModelDefinition): PublicAiCustomModelDefinition { return structuredClone(model) }
