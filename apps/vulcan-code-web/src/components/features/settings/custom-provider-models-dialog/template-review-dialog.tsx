// Review dialog for applying common model templates to custom models.
// 将通用模型模板应用到自定义模型的审阅弹窗。
import * as React from "react"
import { CheckCheck, LoaderCircle, Sparkles } from "lucide-react"
import { CustomProviderMutationContext } from "../custom-provider-mutation-context"
import type { CustomProviderModelRow } from "@/components/features/settings/custom-provider-models-state"
import { Button } from "@/ui-kit/primitives/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from "@/ui-kit/primitives/dialog"
import { ScrollArea } from "@/ui-kit/primitives/scroll-area"
import { PageEmptyState } from "@/components/page"
import { useI18n } from "@/i18n"
import { formatTokenCount } from "@/lib/usage-format"
import { getPublicAiCustomModelTemplateSuggestionsBatch, savePublicAiCustomModelTemplateDecisionsBatch } from "@/service/settings-actions-service"
import type { PublicAiCommonModelTemplateSuggestion, PublicAiCustomModelTemplateDecision, PublicAiCustomModelTemplateDecisionBatchItem, PublicAiCustomModelTemplateSuggestionsRecord, PublicAiSettingsSnapshot } from "@/types"
import { errorMessage } from "@/lib/display-format"
import "@/ui-kit/styles/features/provider-models.css"

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
      <DialogContent className="vk-pm-review-dialog" layout="document" size="xl">
        <header className="vk-viewer-head">
          <span className="vk-viewer-mark"><Sparkles aria-hidden="true" /></span>
          <div className="vk-viewer-titles">
            <DialogTitle>{t("settings.customModels.reviewTemplatesTitle")}</DialogTitle>
            <DialogDescription>{t("settings.customModels.reviewTemplatesDescription")}</DialogDescription>
          </div>
          <div className="vk-pm-counts">
            <TemplateReviewCount label={t("settings.customModels.reviewExact")} value={exactRecords.length} />
            <TemplateReviewCount label={t("settings.customModels.reviewMissing")} value={missingRecords.length} />
          </div>
        </header>
        <ScrollArea>
          <div className="vk-pm-detail-body">
            {loading ? <p className="vk-pm-status"><LoaderCircle aria-hidden="true" className="vk-spin" />{t("settings.customModels.templateSearching")}</p> : null}
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
            {!loading && undecidedRecords.length === 0 ? <PageEmptyState title={t("settings.customModels.reviewComplete")} /> : null}
            {error ? <p className="vk-pm-fault" role="alert">{error}</p> : null}
          </div>
        </ScrollArea>
        <DialogFooter>
          <Button disabled={saving} onClick={() => props.onOpenChange(false)} type="button" variant="outline">{t("memory.common.close")}</Button>
          {exactRecords.length > 0 ? (
            <Button disabled={saving || loading} onClick={() => void saveDecisions(exactRecords.map((record) => ({
              modelId: record.modelId,
              decision: appliedTemplateDecision(record.suggestions[0]),
            })))} type="button">
              {saving ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <CheckCheck aria-hidden="true" />}
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
  return <div className="vk-pm-count"><strong className="vk-num">{props.value}</strong><span>{props.label}</span></div>
}

// TemplateReviewSection groups rows under one human-readable review instruction.
// TemplateReviewSection 将多行按一项人类可读审查说明分组。
function TemplateReviewSection(props: { children: React.ReactNode; title: string }) {
  return <section className="vk-pm-review"><h3 className="vk-label">{props.title}</h3>{props.children}</section>
}

// TemplateReviewRow renders one model and its strongest offline suggestion.
// TemplateReviewRow 渲染一个模型及其最强离线建议。
function TemplateReviewRow(props: { actions?: React.ReactNode; record: PublicAiCustomModelTemplateSuggestionsRecord }) {
  const { t } = useI18n()
  const suggestion = props.record.suggestions[0]
  return (
    <div className="vk-pm-review-row">
      <div>
        <code>{props.record.modelId}</code>
        <small>
          {suggestion
            ? `${suggestion.template_key} · ${formatTokenCount(suggestion.context_limit)} Token · ${t(`settings.customModels.templateMatch.${suggestion.match_kind}`)}`
            : t("settings.customModels.templateDefaultFallback")}
        </small>
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
