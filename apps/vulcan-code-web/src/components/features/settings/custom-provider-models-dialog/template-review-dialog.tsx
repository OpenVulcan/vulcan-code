// Review dialog for applying common model templates to custom models.
// 将通用模型模板应用到自定义模型的审阅弹窗。
import * as React from "react"
import { CheckCheck, LoaderCircle } from "lucide-react"
import { CustomProviderMutationContext } from "../custom-provider-mutation-context"
import type { CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { useI18n } from "@/i18n"
import { formatTokenCount } from "@/lib/usage-format"
import { getPublicAiCustomModelTemplateSuggestionsBatch, savePublicAiCustomModelTemplateDecisionsBatch } from "@/service/settings-actions-service"
import type { PublicAiCommonModelTemplateSuggestion, PublicAiCustomModelTemplateDecision, PublicAiCustomModelTemplateDecisionBatchItem, PublicAiCustomModelTemplateSuggestionsRecord, PublicAiSettingsSnapshot } from "@/types"
import { errorMessage } from "@/lib/display-format"

// TemplateReviewDialog groups deterministic matches and unmatched offline capability records.
// TemplateReviewDialog 对确定性命中与无命中的离线能力记录进行分组。
// Parameters: props supplies provider templates and review actions; returns matched/unmatched groups and guarded explicit application controls.
// 参数：props 提供供应商模板与审阅操作；返回命中和未命中分组及受保护的显式应用控件。
export function TemplateReviewDialog(props: {
  onOpenChange: (open: boolean) => void
  onSaved: (snapshot: PublicAiSettingsSnapshot) => void
  open: boolean
  providerId: string
  rows: CustomProviderModelRow[]
}) {
  const { t } = useI18n()
  // Nested template decisions share the provider dialog's mutation boundary.
  // 嵌套模板决策共享供应商弹窗的变更边界。
  const mutationBoundary = React.useContext(CustomProviderMutationContext)
  const [records, setRecords] = React.useState<PublicAiCustomModelTemplateSuggestionsRecord[]>([])
  const [loading, setLoading] = React.useState(false)
  const [saving, setSaving] = React.useState(false)
  // savingRef protects atomic template decisions from duplicate clicks and accidental closing.
  // savingRef 防止原子模板决策因重复点击或意外关闭而重叠。
  const savingRef = React.useRef(false)
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
      setLoading(false)
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
    if (decisions.length === 0 || savingRef.current) {
      return
    }
    if (mutationBoundary && !mutationBoundary.acquire()) return
    savingRef.current = true
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
      savingRef.current = false
      mutationBoundary?.release()
      setSaving(false)
    }
  }, [mutationBoundary, props])

  return (
    <Dialog onOpenChange={(open) => { if (!savingRef.current) props.onOpenChange(open) }} open={props.open}>
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
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.close")}</Button>
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
            ? `${suggestion.template_key} · ${formatTokenCount(suggestion.context_limit)} Token · ${t(`settings.customModels.templateMatch.${suggestion.match_kind}`)}`
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
