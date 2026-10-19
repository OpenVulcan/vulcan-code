import * as React from "react"
import { LoaderCircle, RotateCcw, Save } from "lucide-react"

import {
  emptyRequestProjection,
  normalizeRequestProjection,
  ReasoningProjectionEditor,
} from "@/components/features/settings/public-ai-model-capability-dialog"

import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { Checkbox } from "@/ui-kit/primitives/checkbox"
import { Input } from "@/ui-kit/primitives/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select"
import { Switch } from "@/ui-kit/primitives/switch"
import {
  directProviderModelSettingsEqual,
  OPENAI_DIRECT_REASONING_EFFORTS,
  projectOpenAiReasoningEfforts,
  resolveDirectCustomProviderDefaults,
} from "@/components/features/settings/direct-provider-model-settings"
import { useI18n } from "@/i18n"
import { CustomProviderMutationContext } from "./custom-provider-mutation-context"
import "@/ui-kit/styles/features/provider-models.css"
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

// CUSTOM_INPUT_RESOURCES lists input kinds with a declared compatible route contract.
// CUSTOM_INPUT_RESOURCES 列出兼容路由已声明契约的输入类型。
const CUSTOM_INPUT_RESOURCES: PublicAiResourceKind[] = ["image", "audio", "file"]

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
// Parameters: props supplies the exact provider and default capability policy; returns numeric drafts and explicit save/reset controls.
// 参数：props 提供精确供应商及默认能力策略；返回数值草稿和显式保存、恢复控件。
export function CustomProviderModelDefaultsCard(props: CustomProviderModelDefaultsCardProps) {
  const { t } = useI18n()
  // mutationBoundary coordinates writes with sibling editors when this card is embedded.
  // mutationBoundary 在本卡片嵌入时与同级编辑器协调写入。
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  // contextLimit is the directly editable provider-wide context value.
  // contextLimit 是可直接编辑的供应商级上下文值。
  const [contextLimit, setContextLimit] = React.useState(String(props.defaults.context_limit))
  // outputLimit is the directly editable provider-wide output-token ceiling.
  // outputLimit 是可直接编辑的供应商级输出 Token 上限。
  const [outputLimit, setOutputLimit] = React.useState(String(props.defaults.output_limit))
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
  const [requestProjection, setRequestProjection] = React.useState<PublicAiCustomModelRequestProjection>(
    normalizeRequestProjection(props.defaults.request_projection) ?? emptyRequestProjection(),
  )
  // saving serializes provider-settings writes from this card.
  // saving 串行化本卡片发起的供应商设置写入。
  const [saving, setSaving] = React.useState(false)
  // savingRef rejects overlapping writes before React updates control state.
  // savingRef 在 React 更新控件状态前拒绝重叠写入。
  const savingRef = React.useRef(false)
  // error contains the current actionable validation or service failure.
  // error 保存当前可操作的校验或服务错误。
  const [error, setError] = React.useState<string | null>(null)

  // resetDraft replaces every field with one authoritative defaults snapshot.
  // resetDraft 使用一份权威默认值快照替换全部字段。
  const resetDraft = React.useCallback((defaults: PublicAiCustomProviderModelDefaults) => {
    setContextLimit(String(defaults.context_limit))
    setOutputLimit(String(defaults.output_limit))
    setInputResources(defaults.input_resources ?? [])
    setStructuredOutput(defaults.structured_output ?? "json_object")
    setReasoningEnabled(reasoningIsEnabled(defaults))
    setReasoningEfforts(visibleReasoningEfforts(defaults))
    setRequestProjection(normalizeRequestProjection(defaults.request_projection))
    setError(null)
  }, [])

  // currentSettings is the complete semantic provider layer currently visible in the editor.
  // currentSettings 是编辑器当前显示的完整语义供应商层。
  const currentSettings = React.useMemo<PublicAiCustomProviderModelDefaults>(() => ({
    context_limit: Number(contextLimit),
    output_limit: Number(outputLimit),
    input_resources: inputResources,
    structured_output: structuredOutput,
    reasoning: reasoningEnabled ? "native" : "unsupported",
    reasoning_control: reasoningEnabled ? "effort_selection" : "fixed_off",
    reasoning_efforts: reasoningEnabled ? reasoningEfforts : ["none"],
    request_projection: requestProjection,
  }), [contextLimit, inputResources, outputLimit, reasoningEfforts, reasoningEnabled, requestProjection, structuredOutput])
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
    if (savingRef.current) return
    if (!Number.isSafeInteger(Number(contextLimit)) || Number(contextLimit) <= 0) {
      setError(t("settings.customModels.defaultsContextInvalid"))
      return
    }
    if (!Number.isSafeInteger(Number(outputLimit)) || Number(outputLimit) <= 0 || Number(outputLimit) > Number(contextLimit)) {
      setError(t("settings.customModels.defaultsOutputInvalid"))
      return
    }
    if (reasoningEnabled && !reasoningEfforts.some((effort) => effort !== "none")) {
      setError(t("settings.customModels.defaultsReasoningInvalid"))
      return
    }
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
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
      savingRef.current = false
      mutationBoundary?.release()
      setSaving(false)
    }
  }, [contextLimit, currentSettings, mutationBoundary, outputLimit, props, reasoningEfforts, reasoningEnabled, t])

  // restore removes the provider layer or discards an unsaved local draft.
  // restore 移除供应商层，或丢弃尚未保存的本地草稿。
  const restore = React.useCallback(async () => {
    if (savingRef.current) return
    if (!props.customized) {
      resetDraft(props.defaults)
      return
    }
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
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
      savingRef.current = false
      mutationBoundary?.release()
      setSaving(false)
    }
  }, [mutationBoundary, props, resetDraft])

  return (
    <section className="vk-pm-box">
      <div className="vk-pm-box-head">
        <div>
          <h3>{t("settings.customModels.defaultsTitle")}</h3>
          <p>{t("settings.customModels.defaultsDescription")}</p>
        </div>
        <Badge variant={props.customized ? "secondary" : "outline"}>
          {t(props.customized
            ? "settings.customModels.defaultsCustomSource"
            : "settings.customModels.defaultsSystemSource")}
        </Badge>
      </div>
      <fieldset disabled={saving} className="vk-fieldset">
        <div className="vk-pm-grid2">
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
        <fieldset className="vk-pm-group">
          <legend>{t("settings.modelDefaults.structuredOutput")}</legend>
          <Select
            onValueChange={(value) => setStructuredOutput(value as PublicAiStructuredOutputCapability)}
            value={structuredOutput}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="json_object">JSON Object</SelectItem>
              <SelectItem value="json_schema">JSON Schema</SelectItem>
              <SelectItem value="unsupported">{t("settings.modelDefaults.unsupported")}</SelectItem>
            </SelectContent>
          </Select>
        </fieldset>
        <p className="vk-field-hint">
          {t("settings.customModels.outputRecommendation").replace(
            "{count}",
            Number.isSafeInteger(Number(contextLimit)) && Number(contextLimit) > 0 ? Math.min(props.defaults.output_limit, Math.floor(Number(contextLimit) / 4)).toLocaleString() : "-",
          )}
        </p>
        <div className="vk-pm-grid2">
          <InputResourceEditor
            label={t("settings.customModels.input")}
            onChange={setInputResources}
            value={inputResources}
          />
          <fieldset className="vk-pm-group">
            <legend>{t("settings.customModels.output")}</legend>
            <strong className="vk-pm-fixed">{t("settings.customModels.fixedTextOutput")}</strong>
            <p className="vk-field-hint">
              {t("settings.customModels.fixedTextOutputDescription")}
            </p>
          </fieldset>
        </div>
        <fieldset className="vk-pm-group">
            <legend>{t("settings.customModels.reasoning")}</legend>
            <label className="vk-pm-switch">
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
            <div className="vk-pm-efforts">
              {OPENAI_DIRECT_REASONING_EFFORTS.map((effort) => (
                <label className="vk-check-label" key={effort}>
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
        <ReasoningProjectionEditor
          efforts={reasoningEfforts.length > 0 ? reasoningEfforts : ["none"]}
          onChange={setRequestProjection}
          projection={requestProjection}
        />
        {error ? <p role="alert" className="vk-field-error">{error}</p> : null}
        <div className="vk-pm-acts" data-spread="">
          <Button
            disabled={saving || (!props.customized && !dirty)}
            onClick={() => void restore()}
            type="button"
            variant="outline"
          >
            <RotateCcw aria-hidden="true" />
            {t("settings.modelCapabilities.restore")}
          </Button>
          <Button disabled={saving || !dirty} onClick={() => void save()} type="button">
            {saving ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <Save aria-hidden="true" />}
            {t("settings.customModels.defaultsSave")}
          </Button>
        </div>
      </fieldset>
    </section>
  )
}

// DirectNumberInput renders one locale-friendly integer field without browser steppers.
// DirectNumberInput 渲染一个不带浏览器步进器且适合本地化的整数输入框。
// Parameters: props supplies the raw numeric draft, label and change handler; returns a plain text-backed integer field preserving invalid input.
// 参数：props 提供原始数值草稿、标签及变更处理器；返回保留非法输入的文本型整数控件。
function DirectNumberInput(props: { label: string; onChange: (value: string) => void; value: string }) {
  return (
    <label className="vk-field">
      {props.label}
      <Input
        inputMode="numeric"
        onChange={(event) => props.onChange(event.target.value)}
        aria-invalid={!Number.isSafeInteger(Number(props.value)) || Number(props.value) <= 0}
        value={props.value}
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
    <fieldset className="vk-pm-group">
      <legend>{props.label}</legend>
      <div className="vk-pm-checks">
        {CUSTOM_INPUT_RESOURCES.map((resource) => (
          <label className="vk-check-label" key={resource}>
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
