import { useI18n } from "@/i18n";
import type { SessionGoalSummary } from "@/types";
import { formatTokenCount } from "@/lib/usage-format";

// ComposerGoalDetails renders an unabridged objective and a two-column usage card from one goal snapshot.
// ComposerGoalDetails 从 goal 快照渲染完整目标与双列用量卡，返回供悬浮层和无障碍读取的内容。
export function ComposerGoalDetails({ goal }: { goal: SessionGoalSummary }) {
  // t and locale keep labels and exact counts consistent with the rest of the composer.
  // t 与 locale 让标签和精确计数与输入区其他部分保持一致。
  const { t, locale } = useI18n();
  // usage excludes cache hits and does not double-count reasoning already contained in output.
  // usage 排除缓存命中，不重复计算已包含在输出中的思考用量。
  const usage = goal.tokensUsed;
  // budgetText belongs exclusively to the hover card, never the compact objective strip.
  // budgetText 仅属于悬浮卡，不再占用紧凑目标条。
  const budgetText = goal.usageKnown === false ? t("composer.goal.usageUnknown") : goal.tokenBudget != null
    ? `${formatTokenCount(usage)} / ${formatTokenCount(goal.tokenBudget)}`
    : formatTokenCount(usage);
  // formatCount keeps request and tool counters exact; token counters use the shared compact token format.
  // formatCount 保持请求与工具计数精确；Token 计数使用共享的紧凑 Token 格式。
  const formatCount = (value: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(Math.max(0, value));
  // metrics pair related counters in the same structure as the existing turn-statistics card.
  // metrics 将相关计数配对，采用现有回合统计卡的布局结构。
  const metrics: Array<[string, string]> = [
    [t("composer.goal.requests"), formatCount(goal.apiCallCount)], [t("composer.goal.tools"), formatCount(goal.toolCallCount)],
    [t("composer.goal.tooltip.input"), formatTokenCount(goal.inputTokens)], [t("composer.goal.tooltip.cache"), formatTokenCount(goal.cachedInputTokens)],
    [t("composer.goal.tooltip.output"), formatTokenCount(goal.outputTokens)], [t("composer.goal.tooltip.reasoning"), formatTokenCount(goal.reasoningTokens)],
  ];
  // input is the complete input denominator; zero input has no defined cache-hit percentage.
  // input 是完整输入分母；输入为零时不存在有效缓存命中率。
  const input = goal.inputTokens + goal.cachedInputTokens;
  // cachePercent preserves the distinction between no samples and a real zero hit rate.
  // cachePercent 区分尚无样本与真实的零命中率。
  const cachePercent = goal.usageKnown !== false && input > 0 ? goal.cachedInputTokens / input * 100 : null;
  return <>
    <div className="vk-stats-body">{goal.objective}</div>
    <div className="vk-sum-title">
      <strong>{t("composer.goal.usage")}</strong><span>{budgetText}</span>
    </div>
    <dl className="vk-stats-grid" data-columns="2" data-goal-metrics>
      {metrics.map(([label, value]) => <div className="vk-stat" key={label}>
        <dt>{label}</dt><dd>{goal.usageKnown === false ? "—" : value}</dd>
      </div>)}
    </dl>
    <div className="vk-stats-section">
      <div className="vk-stats-pair">
        <div className="vk-stat">
          <span>{t("composer.goal.tooltip.total")}</span><strong>{goal.usageKnown === false ? "—" : formatTokenCount(goal.totalTokens)}</strong>
        </div>
        <div className="vk-stat">
          <span>{t("composer.goal.cacheHit")}</span><strong>{cachePercent === null ? "—" : `${cachePercent.toFixed(1)}%`}</strong>
        </div>
      </div>
      <div className="vk-stats-bar" role="progressbar" aria-label={t("composer.goal.cacheHit")}
        aria-valuemin={0} aria-valuemax={100} aria-valuenow={cachePercent ?? undefined}>
        <span style={{ width: `${cachePercent ?? 0}%` }} />
      </div>
    </div>
  </>;
}
