import { useI18n } from "@/i18n";
import type { SessionGoalSummary } from "@/types";
import { formatCompactGoalTokenCount, formatFullGoalTokenCount } from "./goal-token-format";

// ComposerGoalDetails renders an unabridged objective and a two-column usage card from one goal snapshot.
// ComposerGoalDetails 从 goal 快照渲染完整目标与双列用量卡，返回供悬浮层和无障碍读取的内容。
export function ComposerGoalDetails({ goal }: { goal: SessionGoalSummary }) {
  // t and locale keep labels and exact counts consistent with the rest of the composer.
  // t 与 locale 让标签和精确计数与输入区其他部分保持一致。
  const { t, locale } = useI18n();
  // usage excludes cache hits and does not double-count reasoning already contained in output.
  // usage 排除缓存命中，不重复计算已包含在输出中的思考用量。
  const usage = goal.inputTokens + goal.outputTokens;
  // budgetText belongs exclusively to the hover card, never the compact objective strip.
  // budgetText 仅属于悬浮卡，不再占用紧凑目标条。
  const budgetText = goal.tokenBudget != null
    ? `${formatCompactGoalTokenCount(usage)} / ${formatCompactGoalTokenCount(goal.tokenBudget)}`
    : formatCompactGoalTokenCount(usage);
  // metrics pair related counters in the same structure as the existing turn-statistics card.
  // metrics 将相关计数配对，采用现有回合统计卡的布局结构。
  const metrics: Array<[string, number]> = [
    [t("composer.goal.requests"), goal.apiCallCount], [t("composer.goal.tools"), goal.toolCallCount],
    [t("composer.goal.tooltip.input"), goal.inputTokens], [t("composer.goal.tooltip.cache"), goal.cachedInputTokens],
    [t("composer.goal.tooltip.output"), goal.outputTokens], [t("composer.goal.tooltip.reasoning"), goal.reasoningTokens],
  ];
  // input is the complete input denominator; zero input has no defined cache-hit percentage.
  // input 是完整输入分母；输入为零时不存在有效缓存命中率。
  const input = goal.inputTokens + goal.cachedInputTokens;
  // cachePercent preserves the distinction between no samples and a real zero hit rate.
  // cachePercent 区分尚无样本与真实的零命中率。
  const cachePercent = input > 0 ? goal.cachedInputTokens / input * 100 : null;
  return <>
    <div className="composer-goal-details-objective">{goal.objective}</div>
    <div className="context-usage-card-header">
      <strong>{t("composer.goal.usage")}</strong><span>{budgetText}</span>
    </div>
    <dl className="composer-goal-details-metrics">
      {metrics.map(([label, value]) => <div className="turn-stats-metric" key={label}>
        <dt>{label}</dt><dd>{formatFullGoalTokenCount(value, locale)}</dd>
      </div>)}
    </dl>
    <div className="turn-stats-cache-summary">
      <div className="turn-stats-metric-row">
        <div className="turn-stats-metric">
          <span>{t("composer.goal.tooltip.total")}</span><strong>{formatFullGoalTokenCount(goal.totalTokens, locale)}</strong>
        </div>
        <div className="turn-stats-metric">
          <span>{t("composer.goal.cacheHit")}</span><strong>{cachePercent === null ? "—" : `${cachePercent.toFixed(1)}%`}</strong>
        </div>
      </div>
      <div className="turn-stats-cache-track" role="progressbar" aria-label={t("composer.goal.cacheHit")}
        aria-valuemin={0} aria-valuemax={100} aria-valuenow={cachePercent ?? undefined}>
        <div className="turn-stats-cache-fill" style={{ width: `${cachePercent ?? 0}%` }} />
      </div>
    </div>
  </>;
}
