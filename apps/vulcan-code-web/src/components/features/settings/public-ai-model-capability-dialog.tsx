import * as React from "react"
import { LoaderCircle, RotateCcw } from "lucide-react"

import {
  directProviderModelSettingsEqual,
  OPENAI_DIRECT_REASONING_EFFORTS,
  projectOpenAiReasoningEfforts,
  systemProviderCapabilitySettingsDraft,
  systemProviderEditableOverrides,
} from "@/components/features/settings/direct-provider-model-settings"
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
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { useI18n } from "@/i18n"
import {
  savePublicAiModelCapabilityOverrides,
  savePublicAiProviderCapabilityOverrides,
} from "@/service/settings-actions-service"
import type {
  PublicAiContextWindowPolicyOverrides,
  PublicAiCustomModelRequestProjection,
  PublicAiCustomRequestMutation,
  PublicAiModel,
  PublicAiModelCapabilityOverrides,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
  PublicAiStructuredOutputCapability,
} from "@/types"

// ContextWindowPolicyEditor edits one sparse Codex window layer and leaves empty fields inherited.
// ContextWindowPolicyEditor 编辑一层稀疏 Codex 窗口策略，并让空字段继续继承上级。
export function ContextWindowPolicyEditor(props: {
  onChange: (value: PublicAiContextWindowPolicyOverrides | undefined) => void
  value?: PublicAiContextWindowPolicyOverrides
}) {
  const { t } = useI18n()
  // updateNumber preserves the sparse shape while parsing only safe integer input.
  // updateNumber 保持稀疏结构，并且只解析安全整数输入。
  const updateNumber = React.useCallback((
    field: "effective_context_window_percent" | "auto_compact_percent" | "auto_compact_token_limit" | "fallback_buffer_tokens",
    rawValue: string,
  ) => {
    const next: PublicAiContextWindowPolicyOverrides = { ...(props.value ?? {}) }
    const trimmed = rawValue.trim()
    if (!trimmed) {
      delete next[field]
    } else {
      const parsed = Number.parseInt(trimmed, 10)
      if (Number.isSafeInteger(parsed)) {
        next[field] = parsed
      }
    }
    props.onChange(Object.keys(next).length > 0 ? next : undefined)
  }, [props])
  // updateScope changes only the optional usage-scope field.
  // updateScope 只修改可选的用量范围字段。
  const updateScope = React.useCallback((rawValue: string) => {
    const next: PublicAiContextWindowPolicyOverrides = { ...(props.value ?? {}) }
    if (rawValue === "") {
      delete next.auto_compact_scope
    } else {
      next.auto_compact_scope = rawValue as PublicAiContextWindowPolicyOverrides["auto_compact_scope"]
    }
    props.onChange(Object.keys(next).length > 0 ? next : undefined)
  }, [props])
  return (
    <fieldset className="space-y-3 rounded-lg border p-4">
      <legend className="px-1 text-sm font-medium">{t("settings.contextWindowPolicy.title")}</legend>
      <p className="text-xs text-muted-foreground">{t("settings.contextWindowPolicy.description")}</p>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1 text-sm">
          <span>{t("settings.contextWindowPolicy.effectivePercent")}</span>
          <Input
            inputMode="numeric"
            max={100}
            min={1}
            onChange={(event) => updateNumber("effective_context_window_percent", event.target.value)}
            type="number"
            value={props.value?.effective_context_window_percent ?? ""}
          />
        </label>
        <label className="grid gap-1 text-sm">
          <span>{t("settings.contextWindowPolicy.autoCompactPercent")}</span>
          <Input
            inputMode="numeric"
            max={100}
            min={1}
            onChange={(event) => updateNumber("auto_compact_percent", event.target.value)}
            type="number"
            value={props.value?.auto_compact_percent ?? ""}
          />
        </label>
        <label className="grid gap-1 text-sm">
          <span>{t("settings.contextWindowPolicy.autoCompactTokenLimit")}</span>
          <Input
            inputMode="numeric"
            min={1}
            onChange={(event) => updateNumber("auto_compact_token_limit", event.target.value)}
            type="number"
            value={props.value?.auto_compact_token_limit ?? ""}
          />
        </label>
        <label className="grid gap-1 text-sm">
          <span>{t("settings.contextWindowPolicy.scope")}</span>
          <Select
            onValueChange={(value) => updateScope(value === "inherit" ? "" : value)}
            value={props.value?.auto_compact_scope ?? "inherit"}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="inherit">{t("settings.contextWindowPolicy.scopeInherit")}</SelectItem>
              <SelectItem value="total">{t("settings.contextWindowPolicy.scopeTotal")}</SelectItem>
              <SelectItem value="body_after_prefix">{t("settings.contextWindowPolicy.scopeBodyAfterPrefix")}</SelectItem>
            </SelectContent>
          </Select>
        </label>
        <label className="grid gap-1 text-sm">
          <span>{t("settings.contextWindowPolicy.fallbackBuffer")}</span>
          <Input
            inputMode="numeric"
            min={0}
            onChange={(event) => updateNumber("fallback_buffer_tokens", event.target.value)}
            type="number"
            value={props.value?.fallback_buffer_tokens ?? ""}
          />
        </label>
      </div>
      <Button
        className="w-fit"
        disabled={!props.value || Object.keys(props.value).length === 0}
        onClick={() => props.onChange(undefined)}
        type="button"
        variant="outline"
      >
        {t("settings.contextWindowPolicy.clear")}
      </Button>
    </fieldset>
  )
}

// ModelCapabilityDialogProps identifies one model and its optional direct model-settings layer.
// ModelCapabilityDialogProps 标识一个模型及其可选直接模型设置层。
interface ModelCapabilityDialogProps {
  model: PublicAiModel | null
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  overrides?: PublicAiModelCapabilityOverrides
  providerDisplayName: string | null
  providerId: string | null
  // scope selects whether the editor persists a model layer or a provider layer.
  // scope 选择编辑器保存模型层还是供应商层。
  scope?: "model" | "provider"
}

// INPUT_RESOURCE_KINDS is the complete non-text input set editable by this generic dialog.
// INPUT_RESOURCE_KINDS 是此通用对话框可编辑的完整非文本输入集合。
const INPUT_RESOURCE_KINDS: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// REASONING_EFFORT_LABEL_KEYS maps each editable effort to one localized label.
// REASONING_EFFORT_LABEL_KEYS 将每个可编辑档位映射到本地化标签。
const REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  adaptive: "settings.customModels.effortAdaptive",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
  medium: "settings.customModels.effortMedium",
  high: "settings.customModels.effortHigh",
  xhigh: "settings.customModels.effortExtraHigh",
  max: "settings.customModels.effortMax",
  ultra: "settings.customModels.effortUltra",
} as const

// PublicAiModelCapabilityDialog displays effective values and saves only explicitly touched fields.
// PublicAiModelCapabilityDialog 显示有效值，并且只保存用户明确触碰过的字段。
export function PublicAiModelCapabilityDialog(props: ModelCapabilityDialogProps) {
  const { t } = useI18n()
  // isCustomProvider is the exact ownership boundary for editable capability declarations.
  // isCustomProvider 是可编辑能力声明的精确所有权边界。
  const isCustomProvider = props.providerId?.startsWith("custom:") ?? false
  // initializedModelKey prevents snapshot refreshes from overwriting an open editor draft.
  // initializedModelKey 防止快照刷新覆盖已打开编辑器中的草稿。
  const initializedModelKey = React.useRef<string | null>(null)
  // contextLimit is the current direct model context value.
  // contextLimit 是当前直接模型上下文值。
  const [contextLimit, setContextLimit] = React.useState(200_000)
  // outputLimit is the current direct model output-token ceiling.
  // outputLimit 是当前直接模型输出 Token 上限。
  const [outputLimit, setOutputLimit] = React.useState(32_768)
  // inputResources is the complete model-level non-text input set.
  // inputResources 是完整的模型级非文本输入集合。
  const [inputResources, setInputResources] = React.useState<PublicAiResourceKind[]>([])
  // structuredOutput is the model's explicit stable JSON-output contract.
  // structuredOutput 是模型显式声明的稳定 JSON 输出契约。
  const [structuredOutput, setStructuredOutput] = React.useState<PublicAiStructuredOutputCapability>("json_object")
  // reasoningEnabled is the simple user-defined model reasoning policy.
  // reasoningEnabled 是简化的自定义模型思考策略。
  const [reasoningEnabled, setReasoningEnabled] = React.useState(true)
  // reasoningSupport preserves the exact immutable support fact for system providers.
  // reasoningSupport 为系统供应商保留精确且不可变的支持事实。
  const [reasoningSupport, setReasoningSupport] = React.useState<NonNullable<PublicAiModelCapabilityOverrides["reasoning"]>>("native")
  // reasoningControl preserves the exact system contract while custom providers remain directly editable.
  // reasoningControl 保留精确系统契约，同时让自定义供应商保持可直接编辑。
  const [reasoningControl, setReasoningControl] = React.useState("effort_selection")
  // reasoningEfforts is the complete OpenAI-compatible model level set.
  // reasoningEfforts 是完整的 OpenAI 兼容模型档位集合。
  const [reasoningEfforts, setReasoningEfforts] = React.useState<string[]>([])
  // contextWindowPolicy is the sparse Codex threshold layer edited for this scope.
  // contextWindowPolicy 是此作用域正在编辑的稀疏 Codex 阈值层。
  const [contextWindowPolicy, setContextWindowPolicy] = React.useState<PublicAiContextWindowPolicyOverrides | undefined>(undefined)
  const [requestProjection, setRequestProjection] = React.useState<PublicAiCustomModelRequestProjection>(emptyRequestProjection())
  // touchedFields preserves sparse provider edits instead of copying one model's full effective values.
  // touchedFields 保留供应商级稀疏编辑，避免把某一个模型的完整有效值复制给整个供应商。
  const [touchedFields, setTouchedFields] = React.useState<Set<string>>(new Set())
  // edited stays true after a user touches a control even when values return to the baseline.
  // edited 即使值恢复到基线，也会在用户触碰控件后保持为真。
  const [edited, setEdited] = React.useState(false)
  // saving serializes model-settings writes.
  // saving 串行化模型设置写入。
  const [saving, setSaving] = React.useState(false)
  // error contains one current validation or service failure.
  // error 保存当前校验或服务失败。
  const [error, setError] = React.useState<string | null>(null)
  // baselineSettings is the authoritative complete value used for semantic dirty tracking.
  // baselineSettings 是用于语义脏状态跟踪的权威完整值。
  const [baselineSettings, setBaselineSettings] = React.useState<PublicAiModelCapabilityOverrides>({})
  // isCustomProvider gates controls that may override compatible-provider runtime declarations.
  // isCustomProvider 限定可覆盖兼容供应商运行时声明的控件。
  // currentSettings is the complete model layer represented by the visible direct controls.
  // currentSettings 是可见直接控件所表示的完整模型层。
  const currentSettings = React.useMemo<PublicAiModelCapabilityOverrides>(() => ({
    context_limit: contextLimit,
    output_limit: outputLimit,
    input_resources: inputResources,
    structured_output: structuredOutput,
    reasoning: isCustomProvider
      ? (reasoningEnabled ? "native" : "unsupported")
      : reasoningSupport,
    reasoning_control: isCustomProvider
      ? (reasoningEnabled ? reasoningControl : "fixed_off")
      : reasoningControl,
    reasoning_efforts: isCustomProvider
      ? (reasoningEnabled ? reasoningEfforts : ["none"])
      : reasoningEfforts,
    context_window_policy: contextWindowPolicy,
    ...(isCustomProvider ? { request_projection: requestProjection } : {}),
  }), [contextLimit, contextWindowPolicy, inputResources, isCustomProvider, outputLimit, reasoningControl, reasoningEfforts, reasoningEnabled, reasoningSupport, requestProjection, structuredOutput])
  // dirty is true only when the visible model layer differs semantically from its authoritative baseline.
  // dirty 仅在可见模型层与权威基线存在语义差异时为真。
  const dirty = !directProviderModelSettingsEqual(currentSettings, baselineSettings)

  // markField records explicit user ownership for one sparse provider field.
  // markField 记录一个供应商稀疏字段已由用户明确持有。
  const markField = React.useCallback((field: string) => {
    setEdited(true)
    setTouchedFields((current) => new Set(current).add(field))
  }, [])

  // settingsToPersist preserves inheritance by sending a sparse patch for either scope.
  // settingsToPersist 为两个作用域都发送稀疏补丁，从而保留未修改字段的继承关系。
  const settingsToPersist = React.useMemo<PublicAiModelCapabilityOverrides>(() => {
    const patch = buildCapabilityOverridePatch(currentSettings, props.overrides, touchedFields)
    return isCustomProvider ? patch : systemProviderEditableOverrides(patch)
  }, [currentSettings, isCustomProvider, props.overrides, touchedFields])

  // resetDraft projects one complete model layer into the simplified direct controls.
  // resetDraft 将一层完整模型设置投影到简化直接控件。
  const resetDraft = React.useCallback((settings: PublicAiModelCapabilityOverrides) => {
    setContextLimit(settings.context_limit ?? 200_000)
    setOutputLimit(settings.output_limit ?? 32_768)
    setInputResources(settings.input_resources ?? [])
    setStructuredOutput(settings.structured_output ?? "json_object")
    // reasoningEnabled requires both support and control to remain active.
    // reasoningEnabled 要求支持与控制同时保持启用。
    const nextReasoningEnabled = settings.reasoning !== "unsupported"
      && settings.reasoning_control !== "fixed_off"
    setReasoningEnabled(nextReasoningEnabled)
    setReasoningSupport(isCustomProvider
      ? (nextReasoningEnabled ? "native" : "unsupported")
      : (settings.reasoning ?? "unknown"))
    setReasoningControl(settings.reasoning_control ?? (isCustomProvider ? "effort_selection" : "unknown"))
    setReasoningEfforts(isCustomProvider
      ? projectOpenAiReasoningEfforts(settings.reasoning_efforts ?? [])
      : [...(settings.reasoning_efforts ?? [])])
    setContextWindowPolicy(settings.context_window_policy ?? undefined)
    setRequestProjection(normalizeRequestProjection(settings.request_projection))
    setTouchedFields(new Set())
    setEdited(false)
    setError(null)
  }, [isCustomProvider])

  React.useEffect(() => {
    if (!props.open) {
      initializedModelKey.current = null
      return
    }
    if (!props.providerId || !props.model) {
      return
    }
    // modelKey uniquely identifies the draft across provider and model switches.
    // modelKey 在供应商与模型切换之间唯一标识草稿。
    const modelKey = `${props.scope ?? "model"}\u0000${props.providerId}\u0000${props.model.id}`
    // modelChanged forces a complete reset even when the previous model had an unfinished edit.
    // modelChanged 即使前一模型存在未完成编辑，也会强制完整重置。
    const modelChanged = initializedModelKey.current !== modelKey
    if (!modelChanged && (dirty || edited)) {
      return
    }
    // nextSettings resolves explicit values first and then the effective backend model.
    // nextSettings 先解析显式值，再回退到后端有效模型。
    const nextSettings = isCustomProvider
      ? customProviderCapabilitySettingsDraft(props.model, props.overrides)
      : systemProviderCapabilitySettingsDraft(props.model, props.overrides)
    resetDraft(nextSettings)
    setBaselineSettings(nextSettings)
    initializedModelKey.current = modelKey
  }, [dirty, edited, isCustomProvider, props.model, props.open, props.overrides, props.providerId, props.scope, resetDraft])

  // persist saves the complete visible model layer or removes it to restore provider inheritance.
  // persist 保存完整可见模型层，或移除该层以恢复供应商继承。
  const persist = React.useCallback(async (restore: boolean) => {
    if (!props.providerId || !props.model) {
      return
    }
    if (!restore && (!Number.isSafeInteger(contextLimit) || contextLimit <= 0)) {
      setError(t("settings.modelCapabilities.contextInvalid"))
      return
    }
    if (!restore && (!Number.isSafeInteger(outputLimit) || outputLimit <= 0 || outputLimit > contextLimit)) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    if (!restore && isCustomProvider && reasoningEnabled && !reasoningEfforts.some((effort) => effort !== "none")) {
      setError(t("settings.customModels.defaultsReasoningInvalid"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      const nextSnapshot = props.scope === "provider"
        ? await savePublicAiProviderCapabilityOverrides({
            providerId: props.providerId,
            overrides: restore ? undefined : settingsToPersist,
          })
        : await savePublicAiModelCapabilityOverrides({
            providerId: props.providerId,
            modelId: props.model.id,
            overrides: restore ? undefined : settingsToPersist,
          })
      props.onSaved(nextSnapshot)
      props.onOpenChange(false)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [contextLimit, isCustomProvider, outputLimit, props, reasoningEfforts, reasoningEnabled, settingsToPersist, t])

  // restore removes a persisted model layer or discards an unsaved local draft.
  // restore 移除已持久化模型层，或丢弃未保存的本地草稿。
  const restore = React.useCallback(() => {
    if (props.overrides) {
      void persist(true)
      return
    }
    resetDraft(baselineSettings)
  }, [baselineSettings, persist, props.overrides, resetDraft])

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open && saving) {
          return
        }
        props.onOpenChange(open)
      }}
      open={props.open}
    >
      <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-[680px]">
        <DialogHeader>
          <DialogTitle>
            {props.scope === "provider" ? "供应商默认能力" : t("settings.modelCapabilities.title")}
          </DialogTitle>
          <DialogDescription className="flex flex-wrap items-center gap-2">
            <span>
              {props.providerDisplayName} · {props.scope === "provider" ? "全部模型" : props.model?.display_name}
            </span>
            <Badge variant={props.overrides ? "secondary" : "outline"}>
              {props.scope === "provider"
                ? (props.overrides ? "供应商自定义" : "继承目录")
                : t(props.overrides
                  ? "settings.modelCapabilities.modelSource"
                  : "settings.modelCapabilities.inheritedSource")}
            </Badge>
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-4 sm:grid-cols-2">
          <DirectModelNumberInput
            label={t("settings.modelCapabilities.contextLimit")}
            onChange={(value) => { markField("context_limit"); setContextLimit(value) }}
            value={contextLimit}
          />
          <DirectModelNumberInput
            label={t("settings.customModels.outputLimit")}
            onChange={(value) => { markField("output_limit"); setOutputLimit(value) }}
            value={outputLimit}
          />
        </div>
        <ContextWindowPolicyEditor
          onChange={(value) => { markField("context_window_policy"); setContextWindowPolicy(value) }}
          value={contextWindowPolicy}
        />
        {isCustomProvider ? (
          <>
            <InputResourceEditor onChange={(value) => { markField("input_resources"); setInputResources(value) }} value={inputResources} />
            <fieldset className="space-y-2 rounded-lg border p-4">
              <legend className="px-1 text-sm font-medium">结构化输出</legend>
              <Select
                onValueChange={(value) => {
                  markField("structured_output")
                  setStructuredOutput(value as PublicAiStructuredOutputCapability)
                }}
                value={structuredOutput}
              >
                <SelectTrigger><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="json_object">JSON Object</SelectItem>
                  <SelectItem value="json_schema">JSON Schema</SelectItem>
                  <SelectItem value="unsupported">不支持</SelectItem>
                </SelectContent>
              </Select>
              <p className="text-xs text-muted-foreground">未明确声明时默认使用 JSON Object。</p>
            </fieldset>
            <fieldset className="rounded-lg border p-4">
              <legend className="px-1 text-sm font-medium">{t("settings.customModels.output")}</legend>
              <p className="text-sm font-medium">{t("settings.customModels.fixedTextOutput")}</p>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("settings.customModels.fixedTextOutputDescription")}
              </p>
            </fieldset>
            <fieldset className="space-y-3 rounded-lg border p-4">
              <legend className="px-1 text-sm font-medium">{t("settings.customModels.reasoning")}</legend>
              <label className="flex items-center justify-between gap-3 text-sm">
                <span>{t(reasoningEnabled ? "settings.customModels.enabled" : "settings.customModels.disabled")}</span>
                <Switch
                  checked={reasoningEnabled}
                  onCheckedChange={(enabled) => {
                    markField("reasoning")
                    markField("reasoning_control")
                    markField("reasoning_efforts")
                    setReasoningEnabled(enabled)
                    setReasoningSupport(enabled ? "native" : "unsupported")
                    setReasoningControl(enabled ? "effort_selection" : "fixed_off")
                    if (enabled && !reasoningEfforts.some((effort) => effort !== "none")) {
                      setReasoningEfforts([...OPENAI_DIRECT_REASONING_EFFORTS])
                    }
                  }}
                />
              </label>
              <div className="grid grid-cols-2 gap-2">
                {OPENAI_DIRECT_REASONING_EFFORTS.map((effort) => (
                  <label className="flex items-center gap-2 text-xs" key={effort}>
                    <Checkbox
                      checked={reasoningEfforts.includes(effort)}
                      disabled={!reasoningEnabled || (
                        effort !== "none"
                        && reasoningEfforts.includes(effort)
                        && reasoningEfforts.filter((candidate) => candidate !== "none").length === 1
                      )}
                      onCheckedChange={(checked) => {
                        markField("reasoning_efforts")
                        setReasoningEfforts((current) => checked
                          ? [...current, effort]
                          : current.filter((candidate) => candidate !== effort))
                      }}
                    />
                    {t(REASONING_EFFORT_LABEL_KEYS[effort])}
                  </label>
                ))}
              </div>
            </fieldset>
            <ReasoningProjectionEditor
              efforts={reasoningEfforts.length > 0 ? reasoningEfforts : ["none"]}
              onChange={(projection) => { markField("request_projection"); setRequestProjection(projection) }}
              projection={requestProjection}
            />
          </>
        ) : null}
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        <DialogFooter className="sm:justify-between">
          <Button disabled={saving || (!props.overrides && !dirty && !edited)} onClick={restore} type="button" variant="ghost">
            <RotateCcw className="mr-2 size-4" />
            {t("settings.modelCapabilities.restore")}
          </Button>
          <div className="flex justify-end gap-2">
            <Button disabled={saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">
              {t("memory.common.cancel")}
            </Button>
            <Button disabled={saving || (!dirty && !edited)} onClick={() => void persist(false)} type="button">
              {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : null}
              {t("memory.common.save")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// buildCapabilityOverridePatch keeps untouched fields out of the persisted layer.
// buildCapabilityOverridePatch 将未触碰字段排除在持久化层之外。
export function buildCapabilityOverridePatch(
  current: PublicAiModelCapabilityOverrides,
  existing: PublicAiModelCapabilityOverrides | undefined,
  touchedFields: ReadonlySet<string>,
): PublicAiModelCapabilityOverrides {
  const patch: PublicAiModelCapabilityOverrides = { ...(existing ?? {}) }
  const reasoningTouched = ["reasoning", "reasoning_control", "reasoning_efforts"]
    .some((field) => touchedFields.has(field))

  // Reasoning support, control, and levels form one atomic backend contract.
  // 思考支持、控制和档位在后端组成一个原子契约。
  if (reasoningTouched) {
    patch.reasoning = current.reasoning
    patch.reasoning_control = current.reasoning_control
    patch.reasoning_efforts = [...(current.reasoning_efforts ?? [])]
  }
  if (touchedFields.has("context_limit")) {
    patch.context_limit = current.context_limit
  }
  if (touchedFields.has("output_limit")) {
    patch.output_limit = current.output_limit
  }
  if (touchedFields.has("input_resources")) {
    patch.input_resources = [...(current.input_resources ?? [])]
  }
  if (touchedFields.has("structured_output")) {
    patch.structured_output = current.structured_output
  }
  if (touchedFields.has("context_window_policy")) {
    // Null must survive JSON serialization so clearing the editor removes the persisted layer.
    // 清空编辑器时必须保留 null，确保 JSON 序列化后真正移除已持久化的策略层。
    patch.context_window_policy = current.context_window_policy ?? null
  }
  if (touchedFields.has("request_projection")) {
    patch.request_projection = current.request_projection
  }
  return patch
}

// customProviderCapabilitySettingsDraft resolves one complete editable custom-provider layer.
// customProviderCapabilitySettingsDraft 解析一层完整可编辑自定义供应商设置。
function customProviderCapabilitySettingsDraft(
  model: PublicAiModel,
  overrides: PublicAiModelCapabilityOverrides | undefined,
): PublicAiModelCapabilityOverrides {
  // capabilities is the authoritative runtime model after lower-precedence layers resolve.
  // capabilities 是低优先级层解析后的权威运行时模型。
  const capabilities = model.capabilities
  // reasoningSupport preserves explicit model intent before using the effective catalog.
  // reasoningSupport 在使用有效目录前保留显式模型意图。
  const reasoningSupport = overrides?.reasoning ?? capabilities.reasoning
  // reasoningControl preserves the paired control state for explicit disablement.
  // reasoningControl 为显式关闭保留配对控制状态。
  const reasoningControl = overrides?.reasoning_control ?? capabilities.reasoning_control
  // reasoningEnabled requires both support and control to be active.
  // reasoningEnabled 要求支持与控制均处于启用状态。
  const reasoningEnabled = reasoningSupport !== "unsupported" && reasoningControl !== "fixed_off"
  return {
    context_limit: overrides?.context_limit ?? capabilities.context_limit ?? 200_000,
    output_limit: overrides?.output_limit ?? capabilities.output_limit ?? 32_768,
    input_resources: overrides?.input_resources ?? capabilities.input_resources ?? [],
    structured_output: overrides?.structured_output ?? capabilities.structured_output ?? "json_object",
    reasoning: reasoningEnabled ? "native" : "unsupported",
    reasoning_control: reasoningEnabled ? "effort_selection" : "fixed_off",
    reasoning_efforts: reasoningEnabled
      ? projectOpenAiReasoningEfforts(
          overrides?.reasoning_efforts ?? capabilities.reasoning_efforts ?? [],
        )
      : ["none"],
    context_window_policy: overrides?.context_window_policy ?? undefined,
    request_projection: normalizeRequestProjection(overrides?.request_projection),
  }
}

// emptyRequestProjection returns the stable empty shape used by every model editor.
// emptyRequestProjection 返回所有模型编辑器共用的稳定空投影结构。
export function emptyRequestProjection(): PublicAiCustomModelRequestProjection {
  return {
    reasoning: {},
    model_parameter_paths: {},
    defaults: {},
    overrides: {},
    delete: [],
  }
}

// normalizeRequestProjection clones a persisted projection so local edits never mutate a snapshot.
// normalizeRequestProjection 克隆持久化投影，避免本地编辑直接修改快照。
export function normalizeRequestProjection(
  projection: PublicAiCustomModelRequestProjection | undefined,
): PublicAiCustomModelRequestProjection {
  return {
    reasoning: Object.fromEntries(Object.entries(projection?.reasoning ?? {}).map(([effort, mutation]) => [
      effort,
      {
        // Older persisted projections may omit either mutation collection; normalize both at the UI boundary.
        // 旧版持久化投影可能缺少任一变更集合，因此在 UI 边界统一补齐两者。
        set: { ...(mutation?.set ?? {}) },
        delete: [...(mutation?.delete ?? [])],
      },
    ])),
    model_parameter_paths: { ...(projection?.model_parameter_paths ?? {}) },
    defaults: { ...(projection?.defaults ?? {}) },
    overrides: { ...(projection?.overrides ?? {}) },
    delete: [...(projection?.delete ?? [])],
  }
}

// projectionValueText renders a JSON value into an editable single-line representation.
// projectionValueText 将 JSON 值渲染为可编辑的单行表示。
function projectionValueText(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value)
}

// projectionValue parses common JSON literals while retaining ordinary provider strings.
// projectionValue 解析常见 JSON 字面量，同时保留普通供应商字符串。
function projectionValue(value: string): unknown {
  const trimmed = value.trim()
  if (!trimmed) {
    return ""
  }
  try {
    return JSON.parse(trimmed) as unknown
  } catch {
    return value
  }
}

// ReasoningProjectionEditor edits one exact path/value mutation per reasoning effort.
// ReasoningProjectionEditor 编辑每个思考档位的一组精确路径和值变更。
export function ReasoningProjectionEditor(props: {
  efforts: string[]
  onChange: (projection: PublicAiCustomModelRequestProjection) => void
  projection: PublicAiCustomModelRequestProjection
}) {
  const normalizedProjection = normalizeRequestProjection(props.projection)
  const updateMutation = React.useCallback((effort: string, mutation: PublicAiCustomRequestMutation) => {
    const currentProjection = normalizeRequestProjection(props.projection)
    props.onChange({
      ...currentProjection,
      reasoning: {
        ...currentProjection.reasoning,
        [effort]: mutation,
      },
    })
  }, [props])
  return (
    <fieldset className="space-y-3 rounded-lg border p-4">
      <legend className="px-1 text-sm font-medium">请求投影（详细模式）</legend>
      <p className="text-xs text-muted-foreground">
        每个思考档位可配置供应商字段，例如 thinking.type=disabled 或 reasoning_effort=low。
      </p>
      {props.efforts.map((effort) => {
        const mutation = normalizedProjection.reasoning[effort] ?? { set: {}, delete: [] }
        const entries = Object.entries(mutation.set)
        return (
          <div className="space-y-2 rounded-md border p-3" key={effort}>
            <strong className="text-sm">{effort}</strong>
            {entries.map(([path, value], index) => (
              // The row index keeps the input DOM node stable while its editable path changes.
              // 使用行索引保持输入节点稳定，避免编辑字段路径时每个字符都重建输入框并丢失焦点。
              <div className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]" key={`${effort}:set:${index}`}>
                <Input
                  aria-label={`${effort} 字段名`}
                  onChange={(event) => {
                    // Rebuild in the original row order so the stable index key remains stable during typing.
                    // 按原始行顺序重建对象，确保输入过程中稳定的索引 key 不会因字段名变化而改变。
                    const nextSet = Object.fromEntries(entries.map(([candidatePath, candidateValue], candidateIndex) => [
                      candidateIndex === index ? event.target.value : candidatePath,
                      candidateValue,
                    ]))
                    updateMutation(effort, { ...mutation, set: nextSet })
                  }}
                  placeholder="字段名，例如 thinking.type"
                  value={path}
                />
                <Input
                  aria-label={`${effort} 字段值`}
                  onChange={(event) => updateMutation(effort, {
                    ...mutation,
                    set: { ...mutation.set, [path]: projectionValue(event.target.value) },
                  })}
                  placeholder="值，例如 disabled"
                  value={projectionValueText(value)}
                />
                <Button
                  onClick={() => {
                    const nextSet = { ...mutation.set }
                    delete nextSet[path]
                    updateMutation(effort, { ...mutation, set: nextSet })
                  }}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  删除
                </Button>
              </div>
            ))}
            {mutation.delete.map((path, index) => (
              // Keep deletion rows stable while the field path is being typed.
              // 编辑删除路径时保持删除行稳定，避免输入焦点随字符变化而丢失。
              <div className="grid gap-2 sm:grid-cols-[1fr_auto]" key={`${effort}:delete:${index}`}>
                <Input
                  aria-label={`${effort} 删除字段名`}
                  onChange={(event) => {
                    const nextDelete = [...mutation.delete]
                    nextDelete[index] = event.target.value
                    updateMutation(effort, { ...mutation, delete: nextDelete })
                  }}
                  placeholder="删除字段，例如 thinking.type"
                  value={path}
                />
                <Button
                  onClick={() => updateMutation(effort, {
                    ...mutation,
                    delete: mutation.delete.filter((_, candidate) => candidate !== index),
                  })}
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  删除
                </Button>
              </div>
            ))}
            <Button
              onClick={() => updateMutation(effort, {
                ...mutation,
                set: { ...mutation.set, "": "" },
              })}
              size="sm"
              type="button"
              variant="outline"
            >
              添加字段
            </Button>
            <Button
              onClick={() => updateMutation(effort, {
                ...mutation,
                delete: [...mutation.delete, ""],
              })}
              size="sm"
              type="button"
              variant="outline"
            >
              添加删除字段
            </Button>
          </div>
        )
      })}
    </fieldset>
  )
}

// DirectModelNumberInput renders one directly editable positive-integer model setting.
// DirectModelNumberInput 渲染一个可直接编辑的正整数模型设置。
function DirectModelNumberInput(props: { label: string; onChange: (value: number) => void; value: number }) {
  return (
    <label className="grid gap-2 text-sm font-medium">
      {props.label}
      <Input
        inputMode="numeric"
        onChange={(event) => props.onChange(Number.parseInt(event.target.value.replace(/\D/g, "") || "0", 10))}
        value={props.value || ""}
      />
    </label>
  )
}

// InputResourceEditor edits one complete model-level non-text input set without an enable-overrides checkbox.
// InputResourceEditor 在没有“启用覆盖”勾选框的情况下编辑完整模型级非文本输入集合。
function InputResourceEditor(props: {
  onChange: (value: PublicAiResourceKind[]) => void
  value: PublicAiResourceKind[]
}) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-4">
      <legend className="px-1 text-sm font-medium">{t("settings.modelCapabilities.inputResources")}</legend>
      <div className="grid grid-cols-4 gap-2 max-sm:grid-cols-2">
        {INPUT_RESOURCE_KINDS.map((resource) => (
          <label className="flex items-center gap-2 rounded-md border px-3 py-2 text-sm" key={resource}>
            <Checkbox
              checked={props.value.includes(resource)}
              onCheckedChange={(checked) => props.onChange(checked
                ? [...props.value, resource]
                : props.value.filter((candidate) => candidate !== resource))}
            />
            {t(`settings.models.modality.${resource}`)}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
