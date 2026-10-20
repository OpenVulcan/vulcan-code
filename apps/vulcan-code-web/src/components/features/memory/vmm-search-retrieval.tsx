import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemorySearchRetrieval } from "@/types"
import "@/ui-kit/styles/features/memory.css"
import "@/ui-kit/styles/features/settings-forms.css"

// Localize known creation tokens while preserving an unknown server token without guessing its meaning.
// 本地化已知创建标记；保留未知服务端标记，不猜测其含义。
const creationLabels = new Map<string, TranslationKey>([
  ["turn", "memory.nativeRecords.sourceTurnExtracted"], ["direct", "memory.nativeRecords.sourceDirect"],
  ["import", "memory.nativeRecords.sourceImport"], ["system_seed", "memory.nativeRecords.sourceSystemSeed"],
  ["legacy", "memory.nativeRecords.sourceLegacy"],
])

// Shows actual query provenance without confusing creation source or unobserved context with zero evidence.
// 展示实际查询来源，不混淆创建来源，也不将未观察情境当作零证据。
// `retrieval` comes from the server; the component does not infer stages or persist any data.
// `retrieval` 来自服务端；组件不推断阶段，也不持久化数据。
export function VmmSearchRetrieval({ retrieval }: { retrieval: VmmMemorySearchRetrieval }) {
  const { t, locale } = useI18n()
  const sourceLabel = creationLabels.get(retrieval.source_kind)
  // adjustment keeps the nullable server value distinct from zero and rounds only its visible label.
  // adjustment 区分服务端可空值与零，仅对可见标签进行舍入。
  const adjustment = retrieval.matched_context_score_delta
  // roundedAdjustment prevents a rounded negative zero while preserving the exact value in the title.
  // roundedAdjustment 避免舍入后出现负零，同时在标题中保留准确值。
  const roundedAdjustment = adjustment === null ? null : Number(adjustment.toFixed(4))
  return <details className="vk-mem-disclosure vk-form-xs vk-form-break">
    <summary className="vk-truncate vk-form-em" title={`${retrieval.origin_label}\n${retrieval.origin_explanation}`}>{t("memory.content.retrieval")} · {retrieval.origin_label}</summary>
    <dl className="vk-facts vk-form-after">
      <dt>{t("memory.content.finalScore")}</dt><dd>{retrieval.final_score.toFixed(4)}</dd>
      <dt>{t("memory.content.creationSource")}</dt><dd className="vk-truncate" title={sourceLabel ? t(sourceLabel) : retrieval.source_kind}>{sourceLabel ? t(sourceLabel) : retrieval.source_kind}</dd>
      {retrieval.context_observed ? <>
        <dt>{t("memory.content.totalEvidence")}</dt><dd>{retrieval.support_count} / {retrieval.rebuttal_count}</dd>
        <dt>{t("memory.content.matchedEvidence")}</dt><dd>{retrieval.matched_context_support_count} / {retrieval.matched_context_rebuttal_count}</dd>
        <dt>{t("memory.content.contextAdjustment")}</dt><dd title={adjustment === null ? undefined : String(adjustment)}>{roundedAdjustment === null ? "—" : roundedAdjustment === 0 ? "0" : new Intl.NumberFormat(locale, { maximumFractionDigits: 4 }).format(roundedAdjustment)}</dd>
      </> : null}
    </dl>
    {retrieval.context_observed && retrieval.matched_context_values.length > 0
      ? <p className="vk-form-break vk-form-after">{retrieval.matched_context_values.join(" · ")}</p> : null}
    {!retrieval.context_observed ? <p className="vk-faint vk-form-after">{t("memory.content.contextNotObserved")}</p> : null}
  </details>
}
