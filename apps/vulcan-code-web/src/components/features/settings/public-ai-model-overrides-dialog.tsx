import * as React from "react"
import { Database, RotateCcw, SlidersHorizontal } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { useI18n, formatMessage, translateActive } from "@/i18n"
import { savePublicAiModelOverrides } from "@/service/settings-actions-service"
import type {
  PublicAiModel,
  PublicAiModelOverrides,
  PublicAiSettingsSnapshot,
} from "@/types"

// ProtocolModelOverrideDraft is the complete directly editable protocol-settings view.
// ProtocolModelOverrideDraft 是完整的可直接编辑协议设置视图。
interface ProtocolModelOverrideDraft {
  cacheMode: string
  cacheTtl: string
  parameterValues: Record<string, string>
}

// PublicAiModelOverridesDialog edits adapter-declared controls for one provider, model, and protocol.
// PublicAiModelOverridesDialog 编辑一个供应商、模型与协议由适配器声明的控制项。
// Parameters: props identifies one provider/model/protocol binding and its overrides; returns protocol-backed fields and guarded save/reset controls.
// 参数：props 标识一个供应商模型协议绑定及其覆盖；返回基于协议的字段及受保护保存、恢复控件。
export function PublicAiModelOverridesDialog(props: {
  // User-visible provider name.
  // 用户可见供应商名称。
  providerDisplayName: string | null
  // Exact provider identifier.
  // 精确供应商标识。
  providerId: string | null
  // Exact runtime-discovered model.
  // 精确运行时发现模型。
  model: PublicAiModel | null
  // Exact protocol profile identifier.
  // 精确协议配置档标识。
  protocolId: string | null
  // Existing override value.
  // 现有覆盖值。
  overrides?: PublicAiModelOverrides
  // Dialog visibility.
  // 弹窗可见性。
  open: boolean
  // Updates dialog visibility.
  // 更新弹窗可见性。
  onOpenChange: (open: boolean) => void
  // Restores focus to the caller's explicit opener when no DialogTrigger is present.
  // 没有 DialogTrigger 时，将焦点恢复到调用方明确提供的打开按钮。
  onCloseAutoFocus?: React.ComponentProps<typeof DialogContent>["onCloseAutoFocus"]
  // Receives the authoritative settings snapshot after save.
  // 保存后接收权威设置快照。
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
}) {
  const { t } = useI18n()
  const [cacheMode, setCacheMode] = React.useState("provider_default")
  const [cacheTtl, setCacheTtl] = React.useState("")
  const [parameterValues, setParameterValues] = React.useState<Record<string, string>>({})
  const [error, setError] = React.useState<string | null>(null)
  const [saving, setSaving] = React.useState(false)
  // savingRef serializes explicit saves before React publishes the disabled state.
  // savingRef 在 React 发布禁用状态之前串行化显式保存。
  const savingRef = React.useRef(false)
  // baselineDraft is the authoritative route value used for semantic dirty tracking.
  // baselineDraft 是用于语义脏状态跟踪的权威路由值。
  const [baselineDraft, setBaselineDraft] = React.useState<ProtocolModelOverrideDraft>({
    cacheMode: "provider_default",
    cacheTtl: "",
    parameterValues: {},
  })
  // initializedRouteKey prevents unrelated snapshots from erasing an in-progress route edit.
  // initializedRouteKey 防止无关快照清除正在进行的路由编辑。
  const initializedRouteKey = React.useRef<string | null>(null)
  const modelParameters = React.useMemo(
    () => applicableModelParameters(props.model, props.protocolId),
    [props.model, props.protocolId],
  )
  // currentDraft is the complete protocol-settings state represented by visible controls.
  // currentDraft 是可见控件所表示的完整协议设置状态。
  const currentDraft = React.useMemo<ProtocolModelOverrideDraft>(() => ({
    cacheMode,
    cacheTtl,
    parameterValues,
  }), [cacheMode, cacheTtl, parameterValues])
  // dirty is true only when visible route settings differ semantically from the authoritative baseline.
  // dirty 仅在可见路由设置与权威基线存在语义差异时为真。
  const dirty = !protocolModelOverrideDraftEqual(currentDraft, baselineDraft)

  // resetDraft replaces every visible control with one authoritative route value.
  // resetDraft 使用一个权威路由值替换全部可见控件。
  const resetDraft = React.useCallback((draft: ProtocolModelOverrideDraft) => {
    setCacheMode(draft.cacheMode)
    setCacheTtl(draft.cacheTtl)
    setParameterValues(draft.parameterValues)
    setError(null)
  }, [])

  React.useEffect(() => {
    if (!props.open) {
      initializedRouteKey.current = null
      return
    }
    if (!props.providerId || !props.model || !props.protocolId) {
      return
    }
    // routeKey uniquely identifies one provider-model-protocol editor.
    // routeKey 唯一标识一个供应商-模型-协议编辑器。
    const routeKey = `${props.providerId}\u0000${props.model.id}\u0000${props.protocolId}`
    // routeChanged forces a full reset even when the previous route had unfinished edits.
    // routeChanged 即使前一路由存在未完成编辑，也会强制完整重置。
    const routeChanged = initializedRouteKey.current !== routeKey
    if (!routeChanged && dirty) {
      return
    }
    // nextDraft resolves the exact persisted route layer into direct controls.
    // nextDraft 将精确持久化路由层解析为直接控件。
    const nextDraft = protocolModelOverrideDraft(
      props.model,
      props.overrides,
      modelParameters,
    )
    resetDraft(nextDraft)
    setBaselineDraft(nextDraft)
    initializedRouteKey.current = routeKey
  }, [dirty, modelParameters, props.model, props.open, props.overrides, props.protocolId, props.providerId, resetDraft])

  // persist validates browser scalar input and replaces or restores the exact route layer.
  // persist 校验浏览器标量输入，并替换或还原精确路由层。
  const persist = React.useCallback(async (restore: boolean) => {
    if (savingRef.current || !props.providerId || !props.model || !props.protocolId) {
      return
    }
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      const modelParameters: Record<string, unknown> = {}
      for (const definition of restore ? [] : applicableModelParameters(props.model, props.protocolId)) {
        const raw = parameterValues[definition.id]?.trim() ?? ""
        if (!raw) {
          continue
        }
        switch (definition.schema.type) {
          case "boolean":
            if (raw !== "true" && raw !== "false") throw new Error(`${definition.display_name}: ${t("settings.modelExperience.invalidValue")}`)
            modelParameters[definition.id] = { type: "boolean", value: raw === "true" }
            break
          case "integer": {
            const value = Number(raw)
            if (!Number.isSafeInteger(value)) {
              throw new Error(`${definition.display_name}: ${t("settings.modelOverrides.validation.safeInteger")}`)
            }
            modelParameters[definition.id] = { type: "integer", value }
            break
          }
          case "decimal":
            modelParameters[definition.id] = { type: "decimal", value: raw }
            break
          case "enum":
            if (!Array.isArray(definition.schema.values) || !definition.schema.values.includes(raw)) throw new Error(`${definition.display_name}: ${t("settings.modelExperience.invalidValue")}`)
            modelParameters[definition.id] = { type: "string", value: raw }
            break
          case "string":
            modelParameters[definition.id] = { type: "string", value: raw }
            break
          default:
            throw new Error(formatMessage(translateActive("settings.modelOverrides.editorMissing"), { name: definition.display_name }))
        }
      }
      // overrides omits provider-default fields so an empty record restores inheritance.
      // overrides 省略供应商默认字段，使空记录恢复继承。
      if (!restore && cacheMode === "managed_explicit" && (!props.model.capabilities.prompt_cache.supports_managed_explicit || !props.model.capabilities.prompt_cache.supported_ttl_options.some((option) => option.id === cacheTtl))) {
        throw new Error(t("settings.modelExperience.invalidValue"))
      }
      const overrides: PublicAiModelOverrides | undefined = restore ? undefined : {
        ...(cacheMode === "managed_explicit" ? { prompt_cache: {
          mode: "managed_explicit",
          ttl: { type: "capability_option", option_id: cacheTtl },
        } } : {}),
        ...(Object.keys(modelParameters).length > 0 ? { model_parameters: modelParameters } : {}),
      }
      const normalizedOverrides = overrides && Object.keys(overrides).length > 0
        ? overrides
        : undefined
      props.onSaved(await savePublicAiModelOverrides({
        providerId: props.providerId,
        modelId: props.model.id,
        protocolId: props.protocolId,
        overrides: normalizedOverrides,
      }))
      props.onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      setSaving(false)
    }
  }, [
    cacheMode,
    cacheTtl,
    parameterValues,
    props,
    t,
  ])

  // restore removes a persisted route layer or discards an unsaved local draft.
  // restore 移除已持久化路由层，或丢弃未保存的本地草稿。
  const restore = React.useCallback(() => {
    if (props.overrides) {
      void persist(true)
      return
    }
    resetDraft(baselineDraft)
  }, [baselineDraft, persist, props.overrides, resetDraft])

  const promptCache = props.model?.capabilities.prompt_cache
  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!open && savingRef.current) {
          return
        }
        props.onOpenChange(open)
      }}
    >
      <DialogContent className="flex max-h-[calc(100vh-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl" onCloseAutoFocus={props.onCloseAutoFocus}>
        <DialogHeader className="shrink-0 border-b px-5 py-4 pr-12">
          <div className="flex items-start gap-3">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-xl border bg-background shadow-xs">
              <SlidersHorizontal className="size-4 text-muted-foreground" />
            </div>
            <div className="min-w-0 space-y-2">
              <div className="space-y-1">
                <DialogTitle>{t("settings.modelOverrides.title")}</DialogTitle>
                <DialogDescription>{t("settings.modelOverrides.description")}</DialogDescription>
              </div>
              <div className="flex flex-wrap gap-1.5">
                <Badge variant="outline">{props.providerDisplayName}</Badge>
                <Badge variant="secondary">{props.model?.display_name}</Badge>
                <Badge variant="outline">{protocolDisplayName(props.protocolId)}</Badge>
              </div>
            </div>
          </div>
        </DialogHeader>
        <ScrollArea className="min-h-0 flex-1">
          <fieldset disabled={saving} className="min-w-0 space-y-4 p-5">
            <section className="space-y-4">
              <div className="flex items-start gap-2.5">
                <Database className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div className="space-y-1">
                  <h3 className="text-sm font-semibold">{t("settings.modelOverrides.cache")}</h3>
                  <p className="text-xs leading-relaxed text-muted-foreground">
                    {t("settings.modelOverrides.cacheDescription")}
                  </p>
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor="model-override-cache-mode">{t("settings.modelOverrides.cacheMode")}</Label>
                <Select onValueChange={setCacheMode} value={cacheMode}>
                  <SelectTrigger className="w-full" id="model-override-cache-mode">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {cacheMode === "managed_explicit" && !promptCache?.supports_managed_explicit && <SelectItem value="managed_explicit" disabled>{t("settings.modelExperience.invalidValue")}</SelectItem>}
                    <SelectItem value="provider_default">
                      {t("settings.modelOverrides.providerDefault")}
                    </SelectItem>
                    {promptCache?.supports_managed_explicit ? (
                      <SelectItem value="managed_explicit">
                        {t("settings.modelOverrides.managedExplicit")}
                      </SelectItem>
                    ) : null}
                  </SelectContent>
                </Select>
                {promptCache?.provider_default_behavior ? (
                  <p className="rounded-lg bg-muted/50 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
                    {promptCache.provider_default_behavior}
                  </p>
                ) : null}
              </div>
              {cacheMode === "managed_explicit" ? (
                <div className="space-y-2">
                  <Label htmlFor="model-override-cache-ttl">{t("settings.modelOverrides.cacheTtl")}</Label>
                  <Select onValueChange={setCacheTtl} value={cacheTtl}>
                    <SelectTrigger className="w-full" id="model-override-cache-ttl">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {cacheTtl && !promptCache?.supported_ttl_options.some((option) => option.id === cacheTtl) && <SelectItem value={cacheTtl} disabled>{t("settings.modelExperience.invalidValue")}: {cacheTtl}</SelectItem>}
                      {promptCache?.supported_ttl_options.map((option) => (
                        <SelectItem key={option.id} value={option.id}>{option.display_name}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ) : null}
            </section>

            <section className="space-y-4 border-t pt-4">
              <div className="space-y-1">
                <h3 className="text-sm font-semibold">{t("settings.modelOverrides.parameters")}</h3>
                <p className="text-xs leading-relaxed text-muted-foreground">
                  {t("settings.modelOverrides.parametersDescription")}
                </p>
              </div>
              {modelParameters.length > 0 ? <Separator /> : null}
              {modelParameters.length > 0 ? (
                <div className="space-y-4">
                  {modelParameters.map((definition) => (
                    <ModelParameterField
                      definition={definition}
                      key={definition.id}
                      onChange={(value) => setParameterValues((current) => ({
                        ...current,
                        [definition.id]: value,
                      }))}
                      value={parameterValues[definition.id] ?? ""}
                    />
                  ))}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">{t("settings.modelOverrides.noParameters")}</p>
              )}
            </section>

            {error ? (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive" role="alert">
                {error}
              </div>
            ) : null}
          </fieldset>
        </ScrollArea>
        <DialogFooter className="m-0 shrink-0 rounded-none px-5 py-3 sm:justify-between">
          <Button disabled={saving || (!props.overrides && !dirty)} onClick={restore} type="button" variant="ghost">
            <RotateCcw className="mr-2 size-4" />
            {t("settings.modelCapabilities.restore")}
          </Button>
          <div className="flex justify-end gap-2">
            <Button disabled={saving} variant="outline" onClick={() => props.onOpenChange(false)} type="button">
              {t("settings.modelOverrides.cancel")}
            </Button>
            <Button disabled={saving || !dirty} onClick={() => void persist(false)} type="button">
              {saving ? t("settings.modelOverrides.saving") : t("settings.modelOverrides.save")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// protocolModelOverrideDraft resolves one persisted route layer into direct-control values.
// protocolModelOverrideDraft 将一层持久化路由设置解析为直接控件值。
function protocolModelOverrideDraft(
  model: PublicAiModel,
  overrides: PublicAiModelOverrides | undefined,
  modelParameters: PublicAiModel["capabilities"]["model_parameters"],
): ProtocolModelOverrideDraft {
  // promptCache is the only protocol cache shape exposed by the direct editor.
  // promptCache 是直接编辑器公开的唯一协议缓存结构。
  const promptCache = overrides?.prompt_cache as {
    mode?: string
    ttl?: { option_id?: string }
  } | undefined
  return {
    cacheMode: promptCache?.mode === "managed_explicit" ? "managed_explicit" : "provider_default",
    cacheTtl: promptCache?.ttl?.option_id
      ?? model.capabilities.prompt_cache.profile_default_ttl_option_id
      ?? model.capabilities.prompt_cache.supported_ttl_options[0]?.id
      ?? "",
    parameterValues: Object.fromEntries(
      modelParameters.map((definition) => {
        // current is the exact typed persisted parameter value for this adapter declaration.
        // current 是此适配器声明对应的精确已持久化参数值。
        const current = overrides?.model_parameters?.[definition.id] as { value?: unknown } | undefined
        return [definition.id, current?.value === undefined ? "" : String(current.value)]
      }),
    ),
  }
}

// protocolModelOverrideDraftEqual compares the semantic values owned by the protocol editor.
// protocolModelOverrideDraftEqual 比较协议编辑器持有的语义值。
function protocolModelOverrideDraftEqual(
  left: ProtocolModelOverrideDraft,
  right: ProtocolModelOverrideDraft,
): boolean {
  return JSON.stringify(normalizeProtocolModelOverrideDraft(left))
    === JSON.stringify(normalizeProtocolModelOverrideDraft(right))
}

// normalizeProtocolModelOverrideDraft removes ignored TTL and blank-parameter noise.
// normalizeProtocolModelOverrideDraft 移除被忽略的 TTL 与空参数噪声。
function normalizeProtocolModelOverrideDraft(draft: ProtocolModelOverrideDraft) {
  return {
    cacheMode: draft.cacheMode,
    cacheTtl: draft.cacheMode === "managed_explicit" ? draft.cacheTtl : null,
    parameterValues: Object.fromEntries(
      Object.entries(draft.parameterValues)
        .map(([key, value]) => [key, value.trim()] as const)
        .filter(([, value]) => value.length > 0)
        .sort(([left], [right]) => left.localeCompare(right)),
    ),
  }
}

// applicableModelParameters returns only controls registered for the exact model protocol.
// applicableModelParameters 仅返回为精确模型协议注册的控件。
function applicableModelParameters(
  model: PublicAiModel | null,
  protocolId: string | null,
): PublicAiModel["capabilities"]["model_parameters"] {
  if (!model || !protocolId) {
    return []
  }
  return model.capabilities.model_parameters.filter((definition) => {
    const matchesProtocol = definition.protocol_ids.length === 0
      || definition.protocol_ids.includes(protocolId)
    const matchesModel = definition.model_ids.length === 0
      || definition.model_ids.includes(model.id)
    return matchesProtocol && matchesModel
  })
}

// protocolDisplayName maps exact protocol identifiers to user-visible labels.
// protocolDisplayName 把精确协议标识映射为用户可见标签。
function protocolDisplayName(protocolId: string | null): string {
  switch (protocolId) {
    case "anthropic-messages.v1":
      return "Anthropic Messages"
    case "anthropic-messages-video.v1":
      return "Anthropic Messages + Video"
    case "claude-code-messages.v1":
      return "Claude Code Messages"
    case "openai-chat-completions.v1":
      return "OpenAI Chat Completions"
    case "openai-codex-responses.v1":
      return "OpenAI Codex Responses"
    case "openai-responses.v1":
      return "OpenAI Responses"
    default:
      return protocolId ?? ""
  }
}

// ModelParameterField renders one scalar control exactly from the registered schema.
// ModelParameterField 严格根据已注册 Schema 渲染一个标量控件。
// Parameters: props supplies a registered parameter schema and value; returns the appropriate boolean, enum or scalar input.
// 参数：props 提供已注册参数 Schema 与值；返回对应布尔、枚举或标量输入。
function ModelParameterField(props: {
  // Registered parameter definition.
  // 已注册参数定义。
  definition: PublicAiModel["capabilities"]["model_parameters"][number]
  // Current textual form.
  // 当前文本形式。
  value: string
  // Updates the textual form.
  // 更新文本形式。
  onChange: (value: string) => void
}) {
  const { t } = useI18n()
  const fieldId = React.useId()
  const schema = props.definition.schema
  if (schema.type === "boolean") {
    const selectedValue = props.value === "true"
      ? "enabled"
      : props.value === "false"
        ? "disabled"
        : props.value ? "unavailable" : "provider-default"
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
        <Select
          onValueChange={(value) => {
            props.onChange(value === "enabled" ? "true" : value === "disabled" ? "false" : "")
          }}
          value={selectedValue}
        >
          <SelectTrigger className="w-full" id={fieldId}>
            <SelectValue placeholder={t("settings.modelOverrides.providerDefault")} />
          </SelectTrigger>
          <SelectContent>
            {selectedValue === "unavailable" && <SelectItem value="unavailable" disabled>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem>}
            <SelectItem value="provider-default">{t("settings.modelOverrides.providerDefault")}</SelectItem>
            <SelectItem value="enabled">{t("settings.modelOverrides.enabled")}</SelectItem>
            <SelectItem value="disabled">{t("settings.modelOverrides.disabled")}</SelectItem>
          </SelectContent>
        </Select>
        <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  if (schema.type === "enum" && Array.isArray(schema.values)) {
    const values = schema.values as string[]
    const selectedIndex = values.indexOf(props.value)
    return (
      <div className="space-y-2">
        <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
        <Select
          onValueChange={(value) => {
            if (value === "provider-default") {
              props.onChange("")
              return
            }
            const optionIndex = Number(value.slice("option-".length))
            props.onChange(values[optionIndex] ?? "")
          }}
          value={selectedIndex >= 0 ? `option-${selectedIndex}` : props.value ? "unavailable" : "provider-default"}
        >
          <SelectTrigger className="w-full" id={fieldId}>
            <SelectValue placeholder={t("settings.modelOverrides.providerDefault")} />
          </SelectTrigger>
          <SelectContent>
            {selectedIndex < 0 && props.value && <SelectItem value="unavailable" disabled>{t("settings.modelExperience.invalidValue")}: {props.value}</SelectItem>}
            <SelectItem value="provider-default">{t("settings.modelOverrides.providerDefault")}</SelectItem>
            {values.map((value, index) => (
              <SelectItem key={`${index}/${value}`} value={`option-${index}`}>{value}</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
      </div>
    )
  }
  const supported = ["integer", "decimal", "string"].includes(schema.type)
  return (
    <div className="space-y-2">
      <Label htmlFor={fieldId}>{props.definition.display_name}</Label>
      <Input
        disabled={!supported}
        id={fieldId}
        inputMode={schema.type === "integer" ? "numeric" : schema.type === "decimal" ? "decimal" : "text"}
        onChange={(event) => props.onChange(event.currentTarget.value)}
        placeholder={supported
          ? t("settings.modelOverrides.leaveBlank")
          : t("settings.modelOverrides.unsupportedParameter")}
        type="text"
        value={props.value}
      />
      <p className="text-xs leading-relaxed text-muted-foreground">{props.definition.description}</p>
    </div>
  )
}
