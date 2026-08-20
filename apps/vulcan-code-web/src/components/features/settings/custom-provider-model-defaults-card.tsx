import * as React from "react"
import { LoaderCircle, RotateCcw, Save } from "lucide-react"

import {
  ContextWindowPolicyEditor,
  emptyRequestProjection,
  normalizeRequestProjection,
  ReasoningProjectionEditor,
} from "@/components/features/settings/public-ai-model-capability-dialog"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  directProviderModelSettingsEqual,
  OPENAI_DIRECT_REASONING_EFFORTS,
  projectOpenAiReasoningEfforts,
  resolveDirectCustomProviderDefaults,
} from "@/components/features/settings/direct-provider-model-settings"
import { useI18n } from "@/i18n"
import { savePublicAiCustomProviderModelDefaults } from "@/service/settings-actions-service"
import type {
  PublicAiCustomProviderModelDefaults,
  PublicAiCustomModelRequestProjection,
  PublicAiResourceKind,
  PublicAiSettingsSnapshot,
  PublicAiStructuredOutputCapability,
} from "@/types"

// CustomProviderModelDefaultsCardProps binds one direct provider-settings layer to snapshot updates.
// CustomProviderModelDefaultsCardProps 将一层直接供应商设置绑定到快照更新。
interface CustomProviderModelDefaultsCardProps {
  customized: boolean
  defaults: PublicAiCustomProviderModelDefaults
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  providerId: string
}

// CUSTOM_INPUT_RESOURCES is the complete non-text input set editable by compatible providers.
// CUSTOM_INPUT_RESOURCES 是兼容供应商可编辑的完整非文本输入集合。
const CUSTOM_INPUT_RESOURCES: PublicAiResourceKind[] = ["image", "audio", "video", "file"]

// REASONING_EFFORT_LABEL_KEYS maps each OpenAI level to one stable translation key.
// REASONING_EFFORT_LABEL_KEYS 将每个 OpenAI 档位映射到稳定翻译键。
const REASONING_EFFORT_LABEL_KEYS = {
  none: "settings.customModels.effortNone",
  minimal: "settings.customModels.effortMinimal",
  low: "settings.customModels.effortLow",
  medium: "settings.customModels.effortMedium",
  high: "settings.customModels.effortHigh",
  xhigh: "settings.customModels.effortExtraHigh",
  max: "settings.customModels.effortMax",
} as const

// CustomProviderModelDefaultsCard shows current values directly and persists edits without enable-overrides switches.
// CustomProviderModelDefaultsCard 直接显示当前值，并在没有“启用覆盖”开关的情况下持久化修改。
export function CustomProviderModelDefaultsCard(props: CustomProviderModelDefaultsCardProps) {
  const { t } = useI18n()
  // contextLimit is the directly editable provider-wide context value.
  // contextLimit 是可直接编辑的供应商级上下文值。
  const [contextLimit, setContextLimit] = React.useState(props.defaults.context_limit)
  // outputLimit is the directly editable provider-wide output-token ceiling.
  // outputLimit 是可直接编辑的供应商级输出 Token 上限。
  const [outputLimit, setOutputLimit] = React.useState(props.defaults.output_limit)
  // inputResources is the complete provider-wide non-text input set.
  // inputResources 是完整的供应商级非文本输入集合。
  const [inputResources, setInputResources] = React.useState(props.defaults.input_resources ?? [])
  // structuredOutput is the provider-wide stable JSON-output contract.
  // structuredOutput 是供应商级稳定 JSON 输出契约。
  const [structuredOutput, setStructuredOutput] = React.useState<PublicAiStructuredOutputCapability>(props.defaults.structured_output ?? "json_object")
  // reasoningEnabled is the simple explicit on/off policy shown to users.
  // reasoningEnabled 是向用户展示的简化显式思考启停策略。
  const [reasoningEnabled, setReasoningEnabled] = React.useState(reasoningIsEnabled(props.defaults))
  // reasoningEfforts is the complete OpenAI-compatible selectable level set.
  // reasoningEfforts 是完整的 OpenAI 兼容可选档位集合。
  const [reasoningEfforts, setReasoningEfforts] = React.useState<string[]>(
    visibleReasoningEfforts(props.defaults),
  )
  // contextWindowPolicy is the provider-wide sparse Codex threshold layer.
  // contextWindowPolicy 是供应商级稀疏 Codex 阈值层。
  const [contextWindowPolicy, setContextWindowPolicy] = React.useState(props.defaults.context_window_policy)
  const [requestProjection, setRequestProjection] = React.useState<PublicAiCustomModelRequestProjection>(
    normalizeRequestProjection(props.defaults.request_projection) ?? emptyRequestProjection(),
  )
  // saving serializes provider-settings writes from this card.
  // saving 串行化本卡片发起的供应商设置写入。
  const [saving, setSaving] = React.useState(false)
  // error contains the current actionable validation or service failure.
  // error 保存当前可操作的校验或服务错误。
  const [error, setError] = React.useState<string | null>(null)

  // resetDraft replaces every field with one authoritative defaults snapshot.
  // resetDraft 使用一份权威默认值快照替换全部字段。
  const resetDraft = React.useCallback((defaults: PublicAiCustomProviderModelDefaults) => {
    setContextLimit(defaults.context_limit)
    setOutputLimit(defaults.output_limit)
    setInputResources(defaults.input_resources ?? [])
    setStructuredOutput(defaults.structured_output ?? "json_object")
    setReasoningEnabled(reasoningIsEnabled(defaults))
    setReasoningEfforts(visibleReasoningEfforts(defaults))
    setContextWindowPolicy(defaults.context_window_policy)
    setRequestProjection(normalizeRequestProjection(defaults.request_projection))
    setError(null)
  }, [])

  // currentSettings is the complete semantic provider layer currently visible in the editor.
  // currentSettings 是编辑器当前显示的完整语义供应商层。
  const currentSettings = React.useMemo<PublicAiCustomProviderModelDefaults>(() => ({
    context_limit: contextLimit,
    output_limit: outputLimit,
    input_resources: inputResources,
    structured_output: structuredOutput,
    reasoning: reasoningEnabled ? "native" : "unsupported",
    reasoning_control: reasoningEnabled ? "effort_selection" : "fixed_off",
    reasoning_efforts: reasoningEnabled ? reasoningEfforts : ["none"],
    context_window_policy: contextWindowPolicy,
    request_projection: requestProjection,
  }), [contextLimit, contextWindowPolicy, inputResources, outputLimit, reasoningEfforts, reasoningEnabled, requestProjection, structuredOutput])
  // dirty is true only while visible values differ semantically from the authoritative snapshot.
  // dirty 仅在可见值与权威快照存在语义差异时为真。
  const dirty = !directProviderModelSettingsEqual(currentSettings, props.defaults)

  React.useEffect(() => {
    if (!dirty) {
      resetDraft(props.defaults)
    }
  }, [dirty, props.defaults, resetDraft])

  // save validates and replaces the complete visible provider layer in one request.
  // save 校验并通过一次请求替换完整可见供应商层。
  const save = React.useCallback(async () => {
    if (!Number.isSafeInteger(contextLimit) || contextLimit <= 0) {
      setError(t("settings.customModels.defaultsContextInvalid"))
      return
    }
    if (!Number.isSafeInteger(outputLimit) || outputLimit <= 0 || outputLimit > contextLimit) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    if (reasoningEnabled && !reasoningEfforts.some((effort) => effort !== "none")) {
      setError(t("settings.customModels.defaultsReasoningInvalid"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      props.onSaved(await savePublicAiCustomProviderModelDefaults({
        providerId: props.providerId,
        defaults: currentSettings,
      }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [contextLimit, currentSettings, outputLimit, props, reasoningEfforts, reasoningEnabled, t])

  // restore removes the provider layer or discards an unsaved local draft.
  // restore 移除供应商层，或丢弃尚未保存的本地草稿。
  const restore = React.useCallback(async () => {
    if (!props.customized) {
      resetDraft(props.defaults)
      return
    }
    setSaving(true)
    setError(null)
    try {
      // snapshot is the authoritative post-removal state used to replace the stale customized draft immediately.
      // snapshot 是移除后的权威状态，用于立即替换过时的自定义草稿。
      const snapshot = await savePublicAiCustomProviderModelDefaults({ providerId: props.providerId })
      props.onSaved(snapshot)
      resetDraft(resolveDirectCustomProviderDefaults(
        snapshot.customProviderSystemModelDefaults,
        snapshot.customProviderModelDefaults?.[props.providerId],
      ))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      setSaving(false)
    }
  }, [props, resetDraft])

  return (
    <Card>
      <CardHeader className="gap-2 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <CardTitle className="text-sm">{t("settings.customModels.defaultsTitle")}</CardTitle>
            <CardDescription>{t("settings.customModels.defaultsDescription")}</CardDescription>
          </div>
          <Badge variant={props.customized ? "secondary" : "outline"}>
            {t(props.customized
              ? "settings.customModels.defaultsCustomSource"
              : "settings.customModels.defaultsSystemSource")}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-4 border-t p-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <DirectNumberInput
            label={t("settings.customModels.contextLimit")}
            onChange={setContextLimit}
            value={contextLimit}
          />
          <DirectNumberInput
            label={t("settings.customModels.outputLimit")}
            onChange={setOutputLimit}
            value={outputLimit}
          />
        </div>
        <ContextWindowPolicyEditor onChange={setContextWindowPolicy} value={contextWindowPolicy} />
        <fieldset className="space-y-2 rounded-lg border p-3">
          <legend className="px-1 text-xs font-medium">结构化输出</legend>
          <Select
            onValueChange={(value) => setStructuredOutput(value as PublicAiStructuredOutputCapability)}
            value={structuredOutput}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="json_object">JSON Object</SelectItem>
              <SelectItem value="json_schema">JSON Schema</SelectItem>
              <SelectItem value="unsupported">不支持</SelectItem>
            </SelectContent>
          </Select>
        </fieldset>
        <p className="text-xs text-muted-foreground">
          {t("settings.customModels.outputRecommendation").replace(
            "{count}",
            Math.min(32_768, Math.floor(contextLimit / 4)).toLocaleString(),
          )}
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          <InputResourceEditor
            label={t("settings.customModels.input")}
            onChange={setInputResources}
            value={inputResources}
          />
          <fieldset className="rounded-lg border p-3">
            <legend className="px-1 text-xs font-medium">{t("settings.customModels.output")}</legend>
            <p className="text-sm font-medium">{t("settings.customModels.fixedTextOutput")}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("settings.customModels.fixedTextOutputDescription")}
            </p>
          </fieldset>
        </div>
        <div className="grid gap-3">
          <fieldset className="space-y-3 rounded-lg border p-3">
            <legend className="px-1 text-xs font-medium">{t("settings.customModels.reasoning")}</legend>
            <label className="flex items-center justify-between gap-3 text-sm">
              <span>{t(reasoningEnabled ? "settings.customModels.enabled" : "settings.customModels.disabled")}</span>
              <Switch
                checked={reasoningEnabled}
                onCheckedChange={(enabled) => {
                  setReasoningEnabled(enabled)
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
        </div>
        <ReasoningProjectionEditor
          efforts={reasoningEfforts.length > 0 ? reasoningEfforts : ["none"]}
          onChange={setRequestProjection}
          projection={requestProjection}
        />
        {error ? <p className="text-sm text-destructive">{error}</p> : null}
        <div className="flex flex-wrap justify-between gap-2">
          <Button
            disabled={saving || (!props.customized && !dirty)}
            onClick={() => void restore()}
            type="button"
            variant="outline"
          >
            <RotateCcw className="mr-2 size-4" />
            {t("settings.modelCapabilities.restore")}
          </Button>
          <Button disabled={saving || !dirty} onClick={() => void save()} type="button">
            {saving ? <LoaderCircle className="mr-2 size-4 animate-spin" /> : <Save className="mr-2 size-4" />}
            {t("settings.customModels.defaultsSave")}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}

// DirectNumberInput renders one locale-friendly integer field without browser steppers.
// DirectNumberInput 渲染一个不带浏览器步进器且适合本地化的整数输入框。
function DirectNumberInput(props: { label: string; onChange: (value: number) => void; value: number }) {
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

// InputResourceEditor edits one complete provider-level non-text input set.
// InputResourceEditor 编辑一份完整的供应商级非文本输入集合。
function InputResourceEditor(props: {
  label: string
  onChange: (resources: PublicAiResourceKind[]) => void
  value: PublicAiResourceKind[]
}) {
  const { t } = useI18n()
  return (
    <fieldset className="rounded-lg border p-3">
      <legend className="px-1 text-xs font-medium">{props.label}</legend>
      <div className="grid grid-cols-2 gap-2">
        {CUSTOM_INPUT_RESOURCES.map((resource) => (
          <label className="flex items-center gap-2 text-xs" key={resource}>
            <Checkbox
              checked={props.value.includes(resource)}
              onCheckedChange={(checked) => props.onChange(
                checked
                  ? [...props.value, resource]
                  : props.value.filter((candidate) => candidate !== resource),
              )}
            />
            {t(`settings.models.modality.${resource}`)}
          </label>
        ))}
      </div>
    </fieldset>
  )
}

// reasoningIsEnabled converts the complete backend contract into the simple UI policy.
// reasoningIsEnabled 将完整后端契约转换为简化界面策略。
function reasoningIsEnabled(defaults: PublicAiCustomProviderModelDefaults): boolean {
  return defaults.reasoning !== "unsupported" && defaults.reasoning_control !== "fixed_off"
}

// visibleReasoningEfforts projects backend levels onto the OpenAI-compatible editor set.
// visibleReasoningEfforts 将后端档位投影到 OpenAI 兼容编辑器集合。
function visibleReasoningEfforts(defaults: PublicAiCustomProviderModelDefaults): string[] {
  return projectOpenAiReasoningEfforts(defaults.reasoning_efforts)
}
