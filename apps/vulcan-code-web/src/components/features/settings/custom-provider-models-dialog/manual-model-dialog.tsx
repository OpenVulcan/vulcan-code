// Dialog for declaring a custom model manually.
// 手动声明自定义模型的弹窗。
import * as React from "react"
import { LoaderCircle } from "lucide-react"
import { CustomProviderMutationContext } from "../custom-provider-mutation-context"
import { shouldBlockManualModelSubmission } from "@/components/features/settings/custom-provider-models-state"
import { Button } from "@/ui-kit/primitives/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog"
import { Input } from "@/ui-kit/primitives/input"
import { RadioGroup, RadioGroupItem } from "@/ui-kit/primitives/radio-group"
import { useI18n } from "@/i18n"
import { formatTokenCount } from "@/lib/usage-format"
import { getPublicAiCustomModelTemplateSuggestions } from "@/service/settings-actions-service"
import type { PublicAiCommonModelTemplateSuggestion, PublicAiCustomModelDefinition, PublicAiCustomProviderModelDefaults, PublicAiCustomModelTemplateDecision } from "@/types"
import { errorMessage } from "@/lib/display-format"
import { EMPTY_PROJECTION } from "./model-dialog-constants"
import "@/ui-kit/styles/features/provider-models.css"

// ManualModelDialog asks for a real model ID before any model object is created.
// ManualModelDialog 在创建任何模型对象前要求输入真实模型 ID。
// Parameters: props supplies existing model identities and creation callbacks; returns a real-ID form with template selection and duplicate protection.
// 参数：props 提供现有模型身份与创建回调；返回包含真实 ID、模板选择及重复保护的表单。
export function ManualModelDialog(props: {
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
  // Creation shares the provider write boundary while retaining its local close guard.
  // 创建共享供应商写入边界，同时保留局部关闭守卫。
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  const [modelId, setModelId] = React.useState("")
  const [displayName, setDisplayName] = React.useState("")
  const [suggestions, setSuggestions] = React.useState<PublicAiCommonModelTemplateSuggestion[]>([])
  const [choice, setChoice] = React.useState("defaults")
  const [suggestionLoading, setSuggestionLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  // savingRef freezes the exact model identity throughout its atomic creation request.
  // savingRef 在原子创建请求期间冻结精确模型身份。
  const savingRef = React.useRef(false)
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
    if (savingRef.current || suggestionLoading) return
    const id = modelId.trim()
    if (!id) { setError(t("settings.customModels.modelIdRequired")); return }
    if (props.existingIds.has(id)) { setError(t("settings.customModels.duplicateId").replace("{model}", id)); return }
    const selected = suggestions.find((suggestion) => suggestion.template_key === choice)
    const firstSuggestion = suggestions[0]
    const decision = selected ? { decision: "applied" as const, selection: { template_key: selected.template_key, template_revision: selected.template_revision, context_limit: selected.context_limit, supports_text_input: selected.supports_text_input, supports_text_output: selected.supports_text_output, input_resources: selected.input_resources, output_resources: [] } } : firstSuggestion ? { decision: "rejected" as const, template_key: firstSuggestion.template_key, template_revision: firstSuggestion.template_revision } : undefined
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    try { await props.onSaved(newManualModel(id, displayName.trim() || id), decision) } catch (reason) { setError(errorMessage(reason)) } finally { savingRef.current = false; mutationBoundary?.release(); setSaving(false) }
  }, [choice, displayName, modelId, mutationBoundary, props, suggestionLoading, suggestions, t])

  const selectedSuggestion = suggestions.find((suggestion) => suggestion.template_key === choice)
  const summarySource = selectedSuggestion?.template_key ?? t("settings.customModels.templateKeepDefaults")

  return (
    <Dialog onOpenChange={(open) => { if (!savingRef.current) props.onOpenChange(open) }} open={props.open}>
      <DialogContent scrollMode="fixed-footer" size="md">
        <DialogHeader>
          <DialogTitle>{t("settings.customModels.addManual")}</DialogTitle>
          <DialogDescription>{t("settings.customModels.manualDescription")}</DialogDescription>
        </DialogHeader>
        <fieldset disabled={saving} className="vk-fieldset vk-pm-form">
          <label className="vk-field">
            {t("settings.customModels.modelId")}
            <Input autoFocus onChange={(event) => setModelId(event.target.value)} value={modelId} />
          </label>
          <label className="vk-field">
            {t("settings.customModels.displayName")}
            <Input onChange={(event) => setDisplayName(event.target.value)} placeholder={modelId.trim()} value={displayName} />
          </label>
          <fieldset className="vk-pm-group">
            <legend>{t("settings.customModels.templateChoice")}</legend>
            {suggestionLoading ? (
              <p className="vk-pm-status">
                <LoaderCircle aria-hidden="true" className="vk-spin" />
                {t("settings.customModels.templateSearching")}
              </p>
            ) : null}
            <RadioGroup className="vk-pm-choices" onValueChange={setChoice} value={choice}>
              <label className="vk-pm-choice">
                <RadioGroupItem value="defaults" />
                <span>
                  <strong>{t("settings.customModels.templateKeepDefaults")}</strong>
                  <small>{t("settings.customModels.templateDefaultFallback")}</small>
                </span>
              </label>
              {suggestions.map((suggestion) => (
                <label className="vk-pm-choice" key={suggestion.template_key}>
                  <RadioGroupItem value={suggestion.template_key} />
                  <span>
                    <strong>{suggestion.template_key}</strong>
                    <small>
                      {formatTokenCount(suggestion.context_limit)} Token · {t(`settings.customModels.templateMatch.${suggestion.match_kind}`)}
                    </small>
                  </span>
                </label>
              ))}
            </RadioGroup>
          </fieldset>
          <section className="vk-pm-summary">
            <span className="vk-label">{t("settings.customModels.changeSummaryTitle")}</span>
            <dl className="vk-facts">
              <dt>{t("settings.customModels.summaryModel")}</dt>
              <dd className="vk-mono">{modelId.trim() || t("settings.customModels.summaryNotEntered")}</dd>
              <dt>{t("settings.customModels.summaryTemplate")}</dt>
              <dd>{summarySource}</dd>
              <dt>{t("settings.customModels.summaryDefaults")}</dt>
              <dd className="vk-num">
                {formatTokenCount(props.defaults.context_limit)} / {formatTokenCount(props.defaults.output_limit)} Token
              </dd>
            </dl>
          </section>
          {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
        </fieldset>
        <DialogFooter>
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.cancel")}</Button>
          <Button
            disabled={shouldBlockManualModelSubmission(saving, suggestionLoading)}
            onClick={() => void submit()}
            type="button"
          >
            {saving ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : null}
            {t("settings.customModels.addManual")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// newManualModel creates identity and advanced behavior only; defaults and templates fill basic fields.
// newManualModel 仅创建身份与高级行为；基础字段由默认值和模板填充。
export function newManualModel(modelId: string, displayName: string): PublicAiCustomModelDefinition {
  return { id: modelId, display_name: displayName, enabled: true, capabilities: { operations: ["conversation_respond"], input_resources: [], output_resources: [], evidence_revision: "user.manual-model.v2", reasoning: "unknown", reasoning_control: "unknown", reasoning_efforts: [], prompt_cache: { provider_default_behavior: "Follow provider defaults without host-injected cache controls.", supports_managed_explicit: false, supported_ttl_options: [] }, model_parameters: [] }, request_projection: structuredClone(EMPTY_PROJECTION), tool_schema_projection: "lightweight" }
}
