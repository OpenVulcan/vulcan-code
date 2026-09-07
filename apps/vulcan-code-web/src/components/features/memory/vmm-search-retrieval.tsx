import { useI18n, type TranslationKey } from "@/i18n"
import type { VmmMemorySearchRetrieval } from "@/types"

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
  const { t } = useI18n()
  const sourceLabel = creationLabels.get(retrieval.source_kind)
  return <details className="rounded-md border p-2 text-xs">
    <summary className="cursor-pointer font-medium">{t("memory.content.retrieval")} · {retrieval.origin_label}</summary>
    <dl className="mt-2 grid grid-cols-2 gap-2">
      <dt>{t("memory.content.finalScore")}</dt><dd>{retrieval.final_score.toFixed(4)}</dd>
      <dt>{t("memory.content.creationSource")}</dt><dd>{sourceLabel ? t(sourceLabel) : retrieval.source_kind}</dd>
      {retrieval.context_observed ? <>
        <dt>{t("memory.content.totalEvidence")}</dt><dd>{retrieval.support_count} / {retrieval.rebuttal_count}</dd>
        <dt>{t("memory.content.matchedEvidence")}</dt><dd>{retrieval.matched_context_support_count} / {retrieval.matched_context_rebuttal_count}</dd>
        <dt>{t("memory.content.contextAdjustment")}</dt><dd>{retrieval.matched_context_score_delta}</dd>
      </> : null}
    </dl>
    {retrieval.context_observed && retrieval.matched_context_values.length > 0
      ? <p className="mt-2 break-words">{retrieval.matched_context_values.join(" · ")}</p> : null}
    {!retrieval.context_observed ? <p className="mt-2 text-muted-foreground">{t("memory.content.contextNotObserved")}</p> : null}
  </details>
}
