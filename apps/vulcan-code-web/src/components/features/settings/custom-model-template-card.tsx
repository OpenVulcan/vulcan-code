import * as React from "react"
import { Check, LoaderCircle, RotateCcw, Sparkles, X } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/ui-kit/primitives/alert"
import { Badge } from "@/ui-kit/primitives/badge"
import { Button } from "@/ui-kit/primitives/button"
import { useI18n } from "@/i18n"
import { formatTokenCount } from "@/lib/usage-format"
import { CustomProviderMutationContext } from "./custom-provider-mutation-context"
import "@/ui-kit/styles/features/provider-models.css"
import "@/ui-kit/styles/features/settings-forms.css"
import {
  getPublicAiCustomModelTemplateSuggestions,
  savePublicAiCustomModelTemplateDecision,
} from "@/service/settings-actions-service"
import type {
  PublicAiCommonModelTemplateSuggestion,
  PublicAiCustomModelTemplateDecision,
  PublicAiSettingsSnapshot,
} from "@/types"

// CustomModelTemplateCardProps identifies one provider model and its reviewed decision.
// CustomModelTemplateCardProps 标识一个供应商模型及其已审查决策。
interface CustomModelTemplateCardProps {
  decision?: PublicAiCustomModelTemplateDecision
  modelId: string
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  providerId: string
}

// CustomModelTemplateCard presents offline hints without silently applying fuzzy matches.
// CustomModelTemplateCard 展示离线提示且不会静默应用模糊匹配。
// Parameters: props identifies offline model hints and their apply/reject/reset callbacks; returns explicit suggestion actions within the provider mutation boundary.
// 参数：props 标识离线模型提示及应用、拒绝和恢复回调；返回供应商变更边界内的显式建议操作。
export function CustomModelTemplateCard(props: CustomModelTemplateCardProps) {
  const { t } = useI18n()
  const [suggestions, setSuggestions] = React.useState<PublicAiCommonModelTemplateSuggestion[]>([])
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  // The local guard also participates in the containing provider editor's write boundary.
  // 局部守卫同时参与外层供应商编辑器的写入边界。
  const savingRef = React.useRef(false)
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    let active = true
    // Suggestions belong to one model id; a failed read for the next id must not offer the old ones.
    // 建议属于单个模型 ID；下一个 ID 读取失败时不得继续提供旧建议。
    setSuggestions([])
    setLoading(true)
    setError(null)
    const timer = window.setTimeout(() => {
      void getPublicAiCustomModelTemplateSuggestions(props.modelId)
        .then((result) => {
          if (active) {
            setSuggestions(result)
          }
        })
        .catch((reason) => {
          if (active) {
            setError(reason instanceof Error ? reason.message : String(reason))
          }
        })
        .finally(() => {
          if (active) {
            setLoading(false)
          }
        })
    }, 400)
    return () => {
      active = false
      window.clearTimeout(timer)
    }
  }, [props.modelId])

  // saveDecision persists exactly one reviewed immutable template revision.
  // saveDecision 持久化一个精确且已审查的不可变模板修订。
  const saveDecision = React.useCallback(async (
    decision: PublicAiCustomModelTemplateDecision | undefined,
  ) => {
    if (savingRef.current || loading) return
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
    setSaving(true)
    setError(null)
    try {
      const snapshot = await savePublicAiCustomModelTemplateDecision({
        providerId: props.providerId,
        modelId: props.modelId,
        decision,
      })
      props.onSaved(snapshot)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    } finally {
      savingRef.current = false
      mutationBoundary?.release()
      setSaving(false)
    }
  }, [loading, mutationBoundary, props])

  const firstSuggestion = suggestions[0]
  // appliedSelection preserves the discriminated-union narrowing for the reviewed snapshot.
  // appliedSelection 为已审查快照保留可辨识联合类型的收窄结果。
  const appliedSelection = props.decision?.decision === "applied" ? props.decision.selection : undefined
  // appliedSuggestion is the current catalog revision for the already accepted template key.
  // appliedSuggestion 是已接受模板键在当前目录中的修订。
  const appliedSuggestion = appliedSelection
    ? suggestions.find((suggestion) => suggestion.template_key === appliedSelection.template_key)
    : undefined
  if (loading) {
    return <p className="vk-form-status"><LoaderCircle aria-hidden="true" className="vk-spin" />{t("settings.customModels.templateSearching")}</p>
  }
  if (
    appliedSelection
    && appliedSuggestion
    && appliedSuggestion.template_revision !== appliedSelection.template_revision
  ) {
    return (
      <Alert variant="info">
        <Sparkles aria-hidden="true" />
        <AlertTitle>{t("settings.customModels.templateUpdateAvailable")}</AlertTitle>
        <AlertDescription className="vk-form-stack">
          <p>{t("settings.customModels.templateUpdateDescription")}</p>
          <div className="vk-form-codes">
            <p>{t("settings.customModels.templateCurrent")}: {formatTokenCount(appliedSelection.context_limit)} Token · {(appliedSelection.input_resources ?? []).join(", ") || t("settings.models.modality.text")}</p>
            <p>{t("settings.customModels.templateLatest")}: {formatTokenCount(appliedSuggestion.context_limit)} Token · {appliedSuggestion.input_resources.join(", ") || t("settings.models.modality.text")}</p>
          </div>
          <div className="vk-form-inline">
            <Button disabled={saving} onClick={() => void saveDecision(appliedDecision(appliedSuggestion))} size="sm" type="button">
              <Check aria-hidden="true" />{t("settings.customModels.templateApplyUpdate")}
            </Button>
          </div>
          {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
        </AlertDescription>
      </Alert>
    )
  }
  if (props.decision?.decision === "applied") {
    return (
      <Alert variant="success">
        <Sparkles aria-hidden="true" />
        <AlertTitle>{t("settings.customModels.templateApplied")}</AlertTitle>
        <AlertDescription className="vk-form-stack">
          <p>{props.decision.selection.template_key} · {formatTokenCount(props.decision.selection.context_limit)} Token</p>
          <Button disabled={saving} onClick={() => void saveDecision(undefined)} size="sm" type="button" variant="outline">
            <RotateCcw aria-hidden="true" />{t("settings.customModels.templateRestore")}
          </Button>
          {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
        </AlertDescription>
      </Alert>
    )
  }
  if (!firstSuggestion) {
    return (
      <Alert>
        <Sparkles aria-hidden="true" />
        <AlertTitle>{t("settings.customModels.templateNone")}</AlertTitle>
        <AlertDescription>{t("settings.customModels.templateDefaultFallback")}</AlertDescription>
      </Alert>
    )
  }
  if (
    props.decision?.decision === "rejected"
    && props.decision.template_key === firstSuggestion.template_key
    && props.decision.template_revision === firstSuggestion.template_revision
  ) {
    return (
      <Alert>
        <X aria-hidden="true" />
        <AlertTitle>{t("settings.customModels.templateRejected")}</AlertTitle>
        <AlertDescription className="vk-form-stack">
          <p>{t("settings.customModels.templateDefaultFallback")}</p>
          <Button disabled={saving} onClick={() => void saveDecision(undefined)} size="sm" type="button" variant="outline">
            <RotateCcw aria-hidden="true" />{t("settings.customModels.templateReviewAgain")}
          </Button>
        </AlertDescription>
      </Alert>
    )
  }
  return (
    <Alert>
      <Sparkles aria-hidden="true" />
      <AlertTitle className="vk-form-inline">
        {t("settings.customModels.templateSuggested")}
        <Badge variant="secondary">
          {t(`settings.customModels.templateMatch.${firstSuggestion.match_kind}`)}
        </Badge>
      </AlertTitle>
      <AlertDescription className="vk-form-stack">
        <p>{firstSuggestion.template_key} · {formatTokenCount(firstSuggestion.context_limit)} Token</p>
        <div className="vk-form-inline">
          <Button disabled={saving} onClick={() => void saveDecision(appliedDecision(firstSuggestion))} size="sm" type="button">
            <Check aria-hidden="true" />{t("settings.customModels.templateApply")}
          </Button>
          <Button disabled={saving} onClick={() => void saveDecision({
            decision: "rejected",
            template_key: firstSuggestion.template_key,
            template_revision: firstSuggestion.template_revision,
          })} size="sm" type="button" variant="outline">
            {t("settings.customModels.templateKeepDefaults")}
          </Button>
        </div>
        {error ? <p className="vk-field-error" role="alert">{error}</p> : null}
      </AlertDescription>
    </Alert>
  )
}

// appliedDecision snapshots one reviewed immutable suggestion revision.
// appliedDecision 快照一项已审查的不可变建议修订。
function appliedDecision(suggestion: PublicAiCommonModelTemplateSuggestion): PublicAiCustomModelTemplateDecision {
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
