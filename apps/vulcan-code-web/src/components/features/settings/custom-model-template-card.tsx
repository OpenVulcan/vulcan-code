import * as React from "react"
import { Check, LoaderCircle, RotateCcw, Sparkles, X } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { useI18n } from "@/i18n"
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
export function CustomModelTemplateCard(props: CustomModelTemplateCardProps) {
  const { t } = useI18n()
  const [suggestions, setSuggestions] = React.useState<PublicAiCommonModelTemplateSuggestion[]>([])
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)

  React.useEffect(() => {
    let active = true
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
      setSaving(false)
    }
  }, [props])

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
    return <p className="flex items-center gap-2 text-sm text-muted-foreground"><LoaderCircle className="size-4 animate-spin" />{t("settings.customModels.templateSearching")}</p>
  }
  if (
    appliedSelection
    && appliedSuggestion
    && appliedSuggestion.template_revision !== appliedSelection.template_revision
  ) {
    return (
      <Alert>
        <Sparkles className="size-4" />
        <AlertTitle>{t("settings.customModels.templateUpdateAvailable")}</AlertTitle>
        <AlertDescription className="space-y-3">
          <p>{t("settings.customModels.templateUpdateDescription")}</p>
          <div className="grid gap-2 rounded-lg border p-3 text-xs sm:grid-cols-2">
            <p>{t("settings.customModels.templateCurrent")}: {appliedSelection.context_limit.toLocaleString()} Token · {(appliedSelection.input_resources ?? []).join(", ") || t("settings.models.modality.text")}</p>
            <p>{t("settings.customModels.templateLatest")}: {appliedSuggestion.context_limit.toLocaleString()} Token · {appliedSuggestion.input_resources.join(", ") || t("settings.models.modality.text")}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button disabled={saving} onClick={() => void saveDecision(appliedDecision(appliedSuggestion))} size="sm" type="button">
              <Check className="mr-2 size-4" />{t("settings.customModels.templateApplyUpdate")}
            </Button>
          </div>
          {error ? <p className="text-destructive">{error}</p> : null}
        </AlertDescription>
      </Alert>
    )
  }
  if (props.decision?.decision === "applied") {
    return (
      <Alert>
        <Sparkles className="size-4" />
        <AlertTitle>{t("settings.customModels.templateApplied")}</AlertTitle>
        <AlertDescription className="space-y-3">
          <p>{props.decision.selection.template_key} · {props.decision.selection.context_limit.toLocaleString()} Token</p>
          <Button disabled={saving} onClick={() => void saveDecision(undefined)} size="sm" type="button" variant="outline">
            <RotateCcw className="mr-2 size-4" />{t("settings.customModels.templateRestore")}
          </Button>
          {error ? <p className="text-destructive">{error}</p> : null}
        </AlertDescription>
      </Alert>
    )
  }
  if (!firstSuggestion) {
    return (
      <Alert>
        <Sparkles className="size-4" />
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
        <X className="size-4" />
        <AlertTitle>{t("settings.customModels.templateRejected")}</AlertTitle>
        <AlertDescription className="space-y-3">
          <p>{t("settings.customModels.templateDefaultFallback")}</p>
          <Button disabled={saving} onClick={() => void saveDecision(undefined)} size="sm" type="button" variant="outline">
            <RotateCcw className="mr-2 size-4" />{t("settings.customModels.templateReviewAgain")}
          </Button>
        </AlertDescription>
      </Alert>
    )
  }
  return (
    <Alert>
      <Sparkles className="size-4" />
      <AlertTitle className="flex flex-wrap items-center gap-2">
        {t("settings.customModels.templateSuggested")}
        <Badge variant={firstSuggestion.match_kind === "similar" ? "outline" : "secondary"}>
          {t(`settings.customModels.templateMatch.${firstSuggestion.match_kind}`)}
        </Badge>
      </AlertTitle>
      <AlertDescription className="space-y-3">
        <p>{firstSuggestion.template_key} · {firstSuggestion.context_limit.toLocaleString()} Token</p>
        <div className="flex flex-wrap gap-2">
          <Button disabled={saving} onClick={() => void saveDecision(appliedDecision(firstSuggestion))} size="sm" type="button">
            <Check className="mr-2 size-4" />{t("settings.customModels.templateApply")}
          </Button>
          <Button disabled={saving} onClick={() => void saveDecision({
            decision: "rejected",
            template_key: firstSuggestion.template_key,
            template_revision: firstSuggestion.template_revision,
          })} size="sm" type="button" variant="outline">
            {t("settings.customModels.templateKeepDefaults")}
          </Button>
        </div>
        {error ? <p className="text-destructive">{error}</p> : null}
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
      output_resources: suggestion.output_resources,
    },
  }
}
