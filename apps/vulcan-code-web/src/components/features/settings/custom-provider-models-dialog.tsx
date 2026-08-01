import * as React from "react"
import { CheckCheck, DatabaseZap, LoaderCircle, Plus, Search, Trash2, X } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useI18n } from "@/i18n"
import { savePublicAiCustomModels } from "@/service/settings-actions-service"
import type {
  PublicAiCustomModelDefinition,
  PublicAiCustomModelRequestProjection,
  PublicAiModel,
  PublicAiModelParameterDefinition,
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

// SUPPORT_LABEL_KEYS maps serialized support fidelity values to localized user-facing labels.
// SUPPORT_LABEL_KEYS 将序列化支持保真度映射为本地化用户可见标签。
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

// RESOURCE_LABEL_KEYS maps resource kinds to the shared model-modality translations.
// RESOURCE_LABEL_KEYS 将资源类型映射为共享模型模态翻译。
const RESOURCE_LABEL_KEYS = {
  image: "settings.models.modality.image",
  audio: "settings.models.modality.audio",
  video: "settings.models.modality.video",
  file: "settings.models.modality.file",
} as const

// RESOURCE_KINDS is the closed non-text modality set accepted by the public contract.
// RESOURCE_KINDS 是公共契约接受的封闭非文本模态集合。
const RESOURCE_KINDS: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// EMPTY_PROJECTION is the safe request projection used until the user declares exact mappings.
// EMPTY_PROJECTION 是用户声明精确映射前使用的安全请求投影。
const EMPTY_PROJECTION: PublicAiCustomModelRequestProjection = {
  reasoning: {},
  model_parameter_paths: {},
  defaults: {},
  overrides: {},
  delete: [],
}

// CustomProviderModelsDialog manages credential-independent custom model overlays.
// CustomProviderModelsDialog 管理与凭据无关的自定义模型覆盖。
export function CustomProviderModelsDialog(props: CustomProviderModelsDialogProps) {
  const { t } = useI18n()
  const initializedProviderId = React.useRef<string | null>(null)
  const [models, setModels] = React.useState<PublicAiCustomModelDefinition[]>([])
  const [selectedModelId, setSelectedModelId] = React.useState<string | null>(null)
  const [query, setQuery] = React.useState("")
  const [filter, setFilter] = React.useState<CustomModelFilter>("all")
  const [projectionText, setProjectionText] = React.useState("")
  const [parameterText, setParameterText] = React.useState("")
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [removeConfirmationOpen, setRemoveConfirmationOpen] = React.useState(false)
  const [importDialogOpen, setImportDialogOpen] = React.useState(false)
  const [importSelection, setImportSelection] = React.useState<Set<string>>(new Set())

  // discoveredModels deduplicates credential catalogs for this provider without copying accounts.
  // discoveredModels 对当前供应商的凭据目录去重，但不复制账号。
  const discoveredModels = React.useMemo(() => {
    if (!props.provider || !props.snapshot) {
      return []
    }
    const byId = new Map<string, PublicAiModel>()
    for (const connection of props.snapshot.connections) {
      if (connection.provider_id !== props.provider.id) {
        continue
      }
      for (const model of props.snapshot.catalogs[connection.id]?.models ?? []) {
        if (!byId.has(model.id)) {
          byId.set(model.id, model)
        }
      }
    }
    return [...byId.values()].sort((left, right) => left.display_name.localeCompare(right.display_name))
  }, [props.provider, props.snapshot])

  React.useEffect(() => {
    if (!props.open || !props.provider || !props.snapshot) {
      if (!props.open) {
        initializedProviderId.current = null
      }
      return
    }
    if (initializedProviderId.current === props.provider.id) {
      return
    }
    const stored = Object.values(props.snapshot.customModels?.[props.provider.id] ?? {}).map(cloneCustomModel)
    const firstModel = stored[0] ?? null
    setModels(stored)
    setSelectedModelId(firstModel?.id ?? null)
    setProjectionText(JSON.stringify(firstModel?.request_projection ?? EMPTY_PROJECTION, null, 2))
    setParameterText(JSON.stringify(firstModel?.capabilities.model_parameters ?? [], null, 2))
    setQuery("")
    setFilter("all")
    setError(null)
    initializedProviderId.current = props.provider.id
  }, [props.open, props.provider, props.snapshot])

  // selectedModel resolves the current editor target from user-owned state.
  // selectedModel 从用户自有状态解析当前编辑目标。
  const selectedModel = models.find((model) => model.id === selectedModelId) ?? null

  // visibleModels applies search and enabled-state filters without changing the saved set.
  // visibleModels 应用搜索与启用状态筛选，但不改变保存集合。
  const visibleModels = React.useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase()
    return models.filter((model) => {
      if (filter === "enabled" && !model.enabled) {
        return false
      }
      if (filter === "disabled" && model.enabled) {
        return false
      }
      return !normalizedQuery
        || model.id.toLocaleLowerCase().includes(normalizedQuery)
        || model.display_name.toLocaleLowerCase().includes(normalizedQuery)
    })
  }, [filter, models, query])

  // importCandidates excludes models already represented by a custom declaration.
  // importCandidates 排除已由自定义声明表示的模型。
  const importCandidates = React.useMemo(() => {
    const knownIds = new Set(models.map((model) => model.id))
    return discoveredModels.filter((model) => !knownIds.has(model.id))
  }, [discoveredModels, models])

  // commitCurrentAdvanced persists the current JSON editors before any navigation that would replace them.
  // commitCurrentAdvanced 在任何会替换当前 JSON 编辑器的导航前持久化其内容。
  const commitCurrentAdvanced = React.useCallback(() => {
    if (selectedModelId) {
      try {
        const projection = parseProjection(projectionText, t("settings.customModels.projectionInvalid"))
        const parameters = parseArray<PublicAiModelParameterDefinition>(parameterText, t("settings.customModels.parametersInvalid"))
        setModels((current) => current.map((model) => model.id === selectedModelId ? {
          ...model,
          request_projection: projection,
          capabilities: { ...model.capabilities, model_parameters: parameters },
        } : model))
      } catch (reason) {
        setError(errorMessage(reason))
        return false
      }
    }
    setError(null)
    return true
  }, [parameterText, projectionText, selectedModelId, t])

  // selectModel commits valid advanced edits before moving to another model.
  // selectModel 在切换到其他模型前提交有效的高级编辑。
  const selectModel = React.useCallback((nextModelId: string) => {
    if (!commitCurrentAdvanced()) {
      return
    }
    const nextModel = models.find((model) => model.id === nextModelId) ?? null
    setSelectedModelId(nextModelId)
    setProjectionText(JSON.stringify(nextModel?.request_projection ?? EMPTY_PROJECTION, null, 2))
    setParameterText(JSON.stringify(nextModel?.capabilities.model_parameters ?? [], null, 2))
  }, [commitCurrentAdvanced, models])

  // updateSelected replaces one exact model while preserving list order.
  // updateSelected 替换一个精确模型并保留列表顺序。
  const updateSelected = React.useCallback((update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => {
    if (!selectedModelId) {
      return
    }
    setModels((current) => current.map((model) => model.id === selectedModelId ? update(model) : model))
  }, [selectedModelId])

  // renameSelected updates the selection key together with the editable model identifier.
  // renameSelected 在更新可编辑模型标识的同时更新选择键。
  const renameSelected = React.useCallback((modelId: string) => {
    if (!selectedModelId) {
      return
    }
    if (models.some((model) => model.id === modelId && model.id !== selectedModelId)) {
      setError(t("settings.customModels.duplicateId").replace("{model}", modelId))
      return
    }
    const previousModelId = selectedModelId
    setModels((current) => current.map((model) => model.id === previousModelId ? { ...model, id: modelId } : model))
    setSelectedModelId(modelId)
    setError(null)
  }, [models, selectedModelId, t])

  // openImportDialog lets the user explicitly choose synchronized models before importing.
  // openImportDialog 允许用户在导入前明确选择已同步模型。
  const openImportDialog = React.useCallback(() => {
    if (!commitCurrentAdvanced()) {
      return
    }
    if (importCandidates.length === 0) {
      setError(t("settings.customModels.noModelsToImport"))
      return
    }
    setImportSelection(new Set())
    setImportDialogOpen(true)
    setError(null)
  }, [commitCurrentAdvanced, importCandidates.length, t])

  // importDiscovered appends only user-selected synchronized models as editable overlays.
  // importDiscovered 仅把用户选中的已同步模型追加为可编辑覆盖。
  const importDiscovered = React.useCallback(() => {
    const additions = importCandidates
      .filter((model) => importSelection.has(model.id))
      .map(customModelFromDiscovered)
    if (additions.length === 0) {
      return
    }
    setModels((current) => [...current, ...additions])
    setSelectedModelId(additions[0].id)
    setProjectionText(JSON.stringify(additions[0].request_projection, null, 2))
    setParameterText(JSON.stringify(additions[0].capabilities.model_parameters ?? [], null, 2))
    setImportDialogOpen(false)
    setError(null)
  }, [importCandidates, importSelection])

  // setAllEnabled applies one provider-scoped availability value to every custom model.
  // setAllEnabled 将一个供应商范围的可用状态应用到全部自定义模型。
  const setAllEnabled = React.useCallback((enabled: boolean) => {
    if (!commitCurrentAdvanced()) {
      return
    }
    setModels((current) => current.map((model) => ({ ...model, enabled })))
  }, [commitCurrentAdvanced])

  // addManual appends a safe 200k text model draft and focuses it for completion.
  // addManual 追加安全的二十万文本模型草稿，并聚焦供用户补全。
  const addManual = React.useCallback(() => {
    if (!commitCurrentAdvanced()) {
      return
    }
    const occupied = new Set(models.map((model) => model.id))
    let sequence = models.length + 1
    let modelId = `custom-model-${sequence}`
    while (occupied.has(modelId)) {
      sequence += 1
      modelId = `custom-model-${sequence}`
    }
    const model = newCustomModel(modelId)
    setModels((current) => [...current, model])
    setSelectedModelId(model.id)
    setProjectionText(JSON.stringify(model.request_projection, null, 2))
    setParameterText(JSON.stringify(model.capabilities.model_parameters ?? [], null, 2))
    setError(null)
  }, [commitCurrentAdvanced, models])

  // removeSelected removes the editor target after the shared confirmation dialog accepts the action.
  // removeSelected 在共享确认弹窗接受操作后移除编辑目标。
  const removeSelected = React.useCallback(() => {
    if (!selectedModel) {
      return
    }
    const remaining = models.filter((model) => model.id !== selectedModel.id)
    const nextModel = remaining[0] ?? null
    setModels(remaining)
    setSelectedModelId(nextModel?.id ?? null)
    setProjectionText(JSON.stringify(nextModel?.request_projection ?? EMPTY_PROJECTION, null, 2))
    setParameterText(JSON.stringify(nextModel?.capabilities.model_parameters ?? [], null, 2))
    setRemoveConfirmationOpen(false)
  }, [models, selectedModel])

  // save validates editable JSON sections before replacing the provider-owned overlay set.
  // save 在替换供应商自有覆盖集合前校验可编辑 JSON 区域。
  const save = React.useCallback(async () => {
    if (!props.provider) {
      return
    }
    setSaving(true)
    setError(null)
    try {
      const committed = models.map((model) => {
        if (!model.id.trim() || !model.display_name.trim()) {
          throw new Error(t("settings.customModels.identityRequired"))
        }
        if (model.id === selectedModelId) {
          return {
            ...model,
            request_projection: parseProjection(projectionText, t("settings.customModels.projectionInvalid")),
            capabilities: {
              ...model.capabilities,
              model_parameters: parseArray<PublicAiModelParameterDefinition>(parameterText, t("settings.customModels.parametersInvalid")),
            },
          }
        }
        return model
      })
      const uniqueIds = new Set<string>()
      const duplicateId = committed.find((model) => {
        const normalizedId = model.id.trim()
        if (uniqueIds.has(normalizedId)) {
          return true
        }
        uniqueIds.add(normalizedId)
        return false
      })?.id
      if (duplicateId) {
        throw new Error(t("settings.customModels.duplicateId").replace("{model}", duplicateId))
      }
      const nextSnapshot = await savePublicAiCustomModels(props.provider.id, committed)
      props.onSaved(nextSnapshot)
      props.onOpenChange(false)
    } catch (reason) {
      setError(errorMessage(reason))
    } finally {
      setSaving(false)
    }
  }, [models, parameterText, projectionText, props, selectedModelId, t])

  return (
    <Dialog onOpenChange={props.onOpenChange} open={props.open}>
      <DialogContent className="flex h-[min(780px,90vh)] max-w-[min(1120px,94vw)] flex-col gap-0 overflow-hidden p-0">
        <DialogHeader className="border-b px-6 py-4">
          <DialogTitle>{t("settings.customModels.title").replace("{provider}", props.provider?.display_name ?? "")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.description")}</DialogDescription>
        </DialogHeader>
        <div className="grid min-h-0 flex-1 grid-cols-[320px_minmax(0,1fr)] max-md:grid-cols-1">
          <section className="flex min-h-0 flex-col border-r max-md:border-b max-md:border-r-0">
            <div className="space-y-3 border-b p-4">
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  aria-label={t("settings.customModels.search")}
                  className="pl-9"
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t("settings.customModels.search")}
                  value={query}
                />
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
                    <Button aria-label={t("settings.customModels.importDiscovered")} onClick={openImportDialog} size="icon" type="button" variant="outline">
                      <DatabaseZap className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{t("settings.customModels.importDiscovered")}</TooltipContent>
                </Tooltip>
                <Tooltip delayDuration={500}>
                  <TooltipTrigger asChild>
                    <Button aria-label={t("settings.customModels.addManual")} onClick={addManual} size="icon" type="button">
                      <Plus className="size-4" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{t("settings.customModels.addManual")}</TooltipContent>
                </Tooltip>
              </div>
              <div className="grid grid-cols-2 gap-2">
                <Button onClick={() => setAllEnabled(true)} size="sm" type="button" variant="outline">
                  <CheckCheck className="mr-2 size-4" />
                  {t("settings.customModels.enableAll")}
                </Button>
                <Button onClick={() => setAllEnabled(false)} size="sm" type="button" variant="outline">
                  <X className="mr-2 size-4" />
                  {t("settings.customModels.disableAll")}
                </Button>
              </div>
            </div>
            <ScrollArea className="min-h-0 flex-1">
              <div className="space-y-1 p-2">
                {visibleModels.map((model) => (
                  <button
                    className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors ${selectedModelId === model.id ? "bg-accent text-accent-foreground" : "hover:bg-muted"}`}
                    key={model.id}
                    onClick={() => selectModel(model.id)}
                    type="button"
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium">{model.display_name}</span>
                      <span className="block truncate font-mono text-xs text-muted-foreground">{model.id}</span>
                    </span>
                    <Badge variant={model.enabled ? "default" : "secondary"}>
                      {model.enabled ? t("settings.customModels.enabled") : t("settings.customModels.disabled")}
                    </Badge>
                  </button>
                ))}
                {visibleModels.length === 0 ? (
                  <p className="px-3 py-8 text-center text-sm text-muted-foreground">{t("settings.customModels.empty")}</p>
                ) : null}
              </div>
            </ScrollArea>
          </section>
          <ScrollArea className="min-h-0">
            {selectedModel ? (
              <div className="space-y-6 p-6">
                <section className="space-y-4">
                  <div className="flex items-center justify-between gap-4">
                    <div>
                      <h3 className="font-semibold">{t("settings.customModels.basic")}</h3>
                      <p className="text-sm text-muted-foreground">{t("settings.customModels.basicHint")}</p>
                    </div>
                    <div className="flex items-center gap-2 text-sm">
                      <span>{t("settings.customModels.enabled")}</span>
                      <Switch checked={selectedModel.enabled} onCheckedChange={(enabled) => updateSelected((model) => ({ ...model, enabled }))} />
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
                    <LabeledInput label={t("settings.customModels.modelId")} onChange={renameSelected} value={selectedModel.id} />
                    <LabeledInput label={t("settings.customModels.displayName")} onChange={(value) => updateSelected((model) => ({ ...model, display_name: value }))} value={selectedModel.display_name} />
                    <LabeledNumber label={t("settings.customModels.contextLimit")} onChange={(value) => updateSelected((model) => ({ ...model, capabilities: { ...model.capabilities, context_limit: value } }))} value={selectedModel.capabilities.context_limit ?? 200000} />
                    <LabeledNumber label={t("settings.customModels.outputLimit")} optional onChange={(value) => updateSelected((model) => ({ ...model, capabilities: { ...model.capabilities, output_limit: value || undefined } }))} value={selectedModel.capabilities.output_limit ?? 0} />
                  </div>
                </section>
                <Separator />
                <ModalitySection model={selectedModel} update={updateSelected} />
                <Separator />
                <section className="space-y-4">
                  <div>
                    <h3 className="font-semibold">{t("settings.customModels.behavior")}</h3>
                    <p className="text-sm text-muted-foreground">{t("settings.customModels.behaviorHint")}</p>
                  </div>
                  <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
                    <CapabilitySelect label={t("settings.customModels.toolCalling")} onChange={(value) => updateSelected((model) => ({ ...model, capabilities: { ...model.capabilities, tool_calling: value } }))} value={selectedModel.capabilities.tool_calling as ReasoningSupport} />
                    <CapabilitySelect label={t("settings.customModels.parallelTools")} onChange={(value) => updateSelected((model) => ({ ...model, capabilities: { ...model.capabilities, parallel_tool_calling: value } }))} value={(selectedModel.capabilities.parallel_tool_calling as ReasoningSupport | undefined) ?? "unknown"} />
                    <CapabilitySelect label={t("settings.customModels.reasoning")} onChange={(value) => updateSelected((model) => ({ ...model, capabilities: { ...model.capabilities, reasoning: value } }))} value={selectedModel.capabilities.reasoning as ReasoningSupport} />
                    <ReasoningControlSelect model={selectedModel} update={updateSelected} />
                  </div>
                  <div>
                    <p className="mb-2 text-sm font-medium">{t("settings.customModels.reasoningEfforts")}</p>
                    <div className="flex flex-wrap gap-2">
                      {REASONING_EFFORTS.map((effort) => {
                        const checked = (selectedModel.capabilities.reasoning_efforts ?? []).includes(effort)
                        return (
                          <label className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm" key={effort}>
                            <Checkbox checked={checked} onCheckedChange={(next) => updateSelected((model) => ({
                              ...model,
                              capabilities: {
                                ...model.capabilities,
                                reasoning_efforts: next
                                  ? [...(model.capabilities.reasoning_efforts ?? []), effort]
                                  : (model.capabilities.reasoning_efforts ?? []).filter((item) => item !== effort),
                              },
                            }))} />
                            {t(REASONING_EFFORT_LABEL_KEYS[effort])}
                          </label>
                        )
                      })}
                    </div>
                  </div>
                </section>
                <Separator />
                <section className="space-y-4">
                  <Accordion collapsible type="single">
                    <AccordionItem value="advanced">
                      <AccordionTrigger className="py-3 text-left">
                        <span>
                          <span className="block font-semibold">{t("settings.customModels.advanced")}</span>
                          <span className="mt-1 block text-sm font-normal text-muted-foreground">{t("settings.customModels.advancedHint")}</span>
                        </span>
                      </AccordionTrigger>
                      <AccordionContent className="space-y-4 pt-2">
                        <label className="grid gap-2 text-sm font-medium">
                          {t("settings.customModels.parameters")}
                          <Textarea className="min-h-32 font-mono text-xs" onChange={(event) => setParameterText(event.target.value)} spellCheck={false} value={parameterText} />
                        </label>
                        <label className="grid gap-2 text-sm font-medium">
                          {t("settings.customModels.projection")}
                          <Textarea className="min-h-52 font-mono text-xs" onChange={(event) => setProjectionText(event.target.value)} spellCheck={false} value={projectionText} />
                        </label>
                      </AccordionContent>
                    </AccordionItem>
                  </Accordion>
                  <Button className="text-destructive hover:text-destructive" onClick={() => setRemoveConfirmationOpen(true)} type="button" variant="outline">
                    <Trash2 className="mr-2 size-4" />
                    {t("settings.customModels.remove")}
                  </Button>
                </section>
              </div>
            ) : (
              <div className="grid h-full place-items-center p-8 text-center text-sm text-muted-foreground">
                {t("settings.customModels.selectHint")}
              </div>
            )}
          </ScrollArea>
        </div>
        {error ? <p className="border-t bg-destructive/5 px-6 py-3 text-sm text-destructive">{error}</p> : null}
        <DialogFooter className="border-t px-6 py-4">
          <Button onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button>
          <Button disabled={saving} onClick={() => void save()} type="button">
            {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
            {t("settings.customModels.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
      <AlertDialog onOpenChange={setRemoveConfirmationOpen} open={removeConfirmationOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.remove")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.customModels.removeConfirm").replace("{name}", selectedModel?.display_name ?? "")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={removeSelected} variant="destructive">
              {t("settings.customModels.remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
      <AlertDialog onOpenChange={setImportDialogOpen} open={importDialogOpen}>
        <AlertDialogContent className="max-w-xl">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.customModels.importTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("settings.customModels.importDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <ScrollArea className="max-h-80 rounded-lg border">
            <div className="divide-y">
              {importCandidates.map((model) => (
                <label className="flex cursor-pointer items-center gap-3 px-4 py-3 hover:bg-muted/60" key={model.id}>
                  <Checkbox
                    checked={importSelection.has(model.id)}
                    onCheckedChange={(checked) => setImportSelection((current) => {
                      const next = new Set(current)
                      if (checked) {
                        next.add(model.id)
                      } else {
                        next.delete(model.id)
                      }
                      return next
                    })}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{model.display_name}</span>
                    <span className="block truncate font-mono text-xs text-muted-foreground">{model.id}</span>
                  </span>
                </label>
              ))}
            </div>
          </ScrollArea>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("memory.common.cancel")}</AlertDialogCancel>
            <AlertDialogAction disabled={importSelection.size === 0} onClick={importDiscovered}>
              {t("settings.customModels.importSelected").replace("{count}", String(importSelection.size))}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Dialog>
  )
}

// ModalitySection edits only non-text input and output resource declarations.
// ModalitySection 仅编辑非文本输入与输出资源声明。
function ModalitySection(props: {
  model: PublicAiCustomModelDefinition
  update: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void
}) {
  const { t } = useI18n()
  return (
    <section className="space-y-4">
      <div>
        <h3 className="font-semibold">{t("settings.customModels.modalities")}</h3>
        <p className="text-sm text-muted-foreground">{t("settings.customModels.modalitiesHint")}</p>
      </div>
      <div className="grid grid-cols-2 gap-4 max-sm:grid-cols-1">
        <ResourceChecklist label={t("settings.customModels.input")} onChange={(resources) => props.update((model) => ({ ...model, capabilities: { ...model.capabilities, input_resources: resources } }))} value={props.model.capabilities.input_resources ?? []} />
        <ResourceChecklist label={t("settings.customModels.output")} onChange={(resources) => props.update((model) => ({ ...model, capabilities: { ...model.capabilities, output_resources: resources } }))} value={props.model.capabilities.output_resources ?? []} />
      </div>
    </section>
  )
}

// ResourceChecklist renders one compact accessible modality group.
// ResourceChecklist 渲染一个紧凑且可访问的模态分组。
function ResourceChecklist(props: { label: string; onChange: (value: PublicAiResourceKind[]) => void; value: PublicAiResourceKind[] }) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-4">
      <legend className="px-1 text-sm font-medium">{props.label}</legend>
      <div className="grid grid-cols-2 gap-3">
        {RESOURCE_KINDS.map((kind) => (
          <label className="flex items-center gap-2 text-sm" key={kind}>
            <Checkbox checked={props.value.includes(kind)} onCheckedChange={(checked) => props.onChange(checked ? [...props.value, kind] : props.value.filter((item) => item !== kind))} />
            {t(RESOURCE_LABEL_KEYS[kind])}
          </label>
        ))}
      </div>
    </fieldset>
  )
}

// LabeledInput renders one standard text input with a visible label.
// LabeledInput 渲染一个带可见标签的标准文本输入框。
function LabeledInput(props: { label: string; onChange: (value: string) => void; value: string }) {
  return <label className="grid gap-2 text-sm font-medium">{props.label}<Input onChange={(event) => props.onChange(event.target.value)} value={props.value} /></label>
}

// LabeledNumber renders one direct numeric input without spinner stepping controls.
// LabeledNumber 渲染一个不依赖步进操作的直接数值输入框。
function LabeledNumber(props: { label: string; onChange: (value: number) => void; optional?: boolean; value: number }) {
  return <label className="grid gap-2 text-sm font-medium">{props.label}<Input inputMode="numeric" onChange={(event) => props.onChange(Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))} value={props.value || ""} /></label>
}

// CapabilitySelect renders the closed provider-neutral support fidelity values.
// CapabilitySelect 渲染封闭的供应商无关支持保真度值。
function CapabilitySelect(props: { label: string; onChange: (value: ReasoningSupport) => void; value: ReasoningSupport }) {
  const { t } = useI18n()
  return (
    <label className="grid gap-2 text-sm font-medium">
      {props.label}
      <Select onValueChange={(value) => props.onChange(value as ReasoningSupport)} value={props.value}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>{(Object.keys(SUPPORT_LABEL_KEYS) as ReasoningSupport[]).map((value) => <SelectItem key={value} value={value}>{t(SUPPORT_LABEL_KEYS[value])}</SelectItem>)}</SelectContent>
      </Select>
    </label>
  )
}

// ReasoningControlSelect edits the exact reasoning-control contract.
// ReasoningControlSelect 编辑精确的推理控制契约。
function ReasoningControlSelect(props: {
  model: PublicAiCustomModelDefinition
  update: (update: (model: PublicAiCustomModelDefinition) => PublicAiCustomModelDefinition) => void
}) {
  const { t } = useI18n()
  const values = Object.keys(REASONING_CONTROL_LABEL_KEYS) as Array<keyof typeof REASONING_CONTROL_LABEL_KEYS>
  return (
    <label className="grid gap-2 text-sm font-medium">
      {t("settings.customModels.reasoningControl")}
      <Select onValueChange={(value) => props.update((model) => ({ ...model, capabilities: { ...model.capabilities, reasoning_control: value } }))} value={props.model.capabilities.reasoning_control ?? "unknown"}>
        <SelectTrigger><SelectValue /></SelectTrigger>
        <SelectContent>{values.map((value) => <SelectItem key={value} value={value}>{t(REASONING_CONTROL_LABEL_KEYS[value])}</SelectItem>)}</SelectContent>
      </Select>
    </label>
  )
}

// customModelFromDiscovered copies immutable discovered capabilities into a user-owned draft.
// customModelFromDiscovered 把不可变发现能力复制到用户自有草稿中。
function customModelFromDiscovered(model: PublicAiModel): PublicAiCustomModelDefinition {
  return {
    id: model.id,
    display_name: model.display_name,
    enabled: true,
    capabilities: structuredClone(model.capabilities),
    request_projection: structuredClone(EMPTY_PROJECTION),
  }
}

// newCustomModel creates the safe fallback draft used for manual model entry.
// newCustomModel 创建手动录入模型使用的安全回退草稿。
function newCustomModel(modelId: string): PublicAiCustomModelDefinition {
  return {
    id: modelId,
    display_name: modelId,
    enabled: true,
    capabilities: {
      operations: ["conversation_respond"],
      context_limit: 200000,
      input_resources: [],
      output_resources: [],
      evidence_revision: "user.custom-model.v1",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      tool_calling: "unknown",
      parallel_tool_calling: "unknown",
      prompt_cache: {
        provider_default_behavior: "Follow provider defaults without host-injected cache controls.",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
    },
    request_projection: structuredClone(EMPTY_PROJECTION),
  }
}

// cloneCustomModel creates an isolated editable copy of one persisted model definition.
// cloneCustomModel 创建一个持久化模型定义的隔离可编辑副本。
function cloneCustomModel(model: PublicAiCustomModelDefinition): PublicAiCustomModelDefinition {
  return structuredClone(model)
}

// parseProjection validates the advanced request projection JSON object.
// parseProjection 校验高级请求投影 JSON 对象。
function parseProjection(value: string, invalidMessage: string): PublicAiCustomModelRequestProjection {
  const parsed = JSON.parse(value) as unknown
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(invalidMessage)
  }
  return parsed as PublicAiCustomModelRequestProjection
}

// parseArray validates one advanced JSON array with a localized failure message.
// parseArray 使用本地化失败消息校验一个高级 JSON 数组。
function parseArray<Item>(value: string, message: string): Item[] {
  const parsed = JSON.parse(value) as unknown
  if (!Array.isArray(parsed)) {
    throw new Error(message)
  }
  return parsed as Item[]
}

// errorMessage normalizes rejected service and JSON errors for display.
// errorMessage 规范化服务拒绝与 JSON 错误以供显示。
function errorMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason)
}
