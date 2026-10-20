import { formatMessage, useI18n } from "@/i18n";
import { formatDurationSeconds, formatTokenCount, formatTokenSpeed, tokenUnitLabel } from "@/lib/usage-format";
import { ChevronDown } from "lucide-react";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import type { SessionLifetimeUsage, SessionUsageBucket, UsageMeasurement } from "@/types/session-lifetime-usage";

/** Formats a measured total, preserving unknown and partial reporting.
 * 格式化实测累计，保留未知与部分上报。
 * Returns a display string for one measurement and expected request count.
 * 返回一项测量与预期请求数对应的显示文字。 */
export function lifetimeTotal(value: UsageMeasurement, requests: number): string {
  if (requests === 0) return "0";
  if (value.samples === 0) return "—";
  return `${value.samples < requests ? "≥ " : ""}${formatTokenCount(value.sum)}`;
}

/** Renders a collapsed aggregate row whose heading toggles all measured statistics.
 * 展示默认收起的累计行，点击标题行切换全部实测统计。
 * Props contain a title and an optional measured bucket; returns a native disclosure section.
 * 属性包含标题与可选实测桶；返回原生折叠统计区块。 */
export function LifetimeUsageBucket(props: { title: string; bucket?: SessionUsageBucket }) {
  // Locale controls labels only; the backend owns every counter.
  // 语言仅控制文案；所有计数由后端持有。
  const { locale, t } = useI18n();
  const bucket = props.bucket;
  if (!bucket || (bucket.requestCount === 0 && bucket.toolCallCount === 0)) return null;
  // Averages use reporting samples and never treat missing timing as zero.
  // 均值使用已上报样本，不将缺失计时视为零。
  const first = bucket.firstTokenMs.samples > 0 && bucket.firstTokenMs.sum > 0 ? formatDurationSeconds(bucket.firstTokenMs.sum / bucket.firstTokenMs.samples) : null;
  const speed = bucket.outputDurationMs.samples > 0 && bucket.outputDurationMs.sum > 0 && bucket.measuredOutputTokens > 0 ? formatTokenSpeed(bucket.measuredOutputTokens * 1000 / bucket.outputDurationMs.sum, locale) : null;
  // Keep partial totals explicit, but omit zero and unreported token dimensions from the visible layout.
  // 保留部分上报累计的明确标记，但不让零值及未上报维度占据界面。
  const tokenValue = (value: UsageMeasurement): string | null => value.samples > 0 && value.sum > 0
    ? lifetimeTotal(value, bucket.requestCount) : null;
  // Group counters separately from token measurements and timing to keep related facts adjacent.
  // 将调用次数、Token 测量和性能分别归类，让相关指标保持相邻。
  const callRows: Array<[string, string | null]> = [
    [t("conversation.usage.requests"), bucket.requestCount > 0 ? bucket.requestCount.toLocaleString() : null],
    [t("conversation.usage.toolCalls"), bucket.toolCallCount > 0 ? bucket.toolCallCount.toLocaleString() : null],
  ];
  // Token rows retain the original measurement and missing-value semantics.
  // Token 分组保留原有测量与缺失值语义。
  const tokenRows: Array<[string, string | null]> = [
    [t("conversation.usage.input"), tokenValue(bucket.inputTokens)],
    [t("conversation.usage.output"), tokenValue(bucket.outputTokens)],
    [t("conversation.usage.uncachedInput"), tokenValue(bucket.uncachedInputTokens)],
    [t("conversation.usage.cacheRead"), tokenValue(bucket.cacheReadTokens)],
    [t("conversation.usage.cacheWrite"), tokenValue(bucket.cacheWriteTokens)],
    [t("conversation.usage.reasoning"), tokenValue(bucket.reasoningTokens)],
  ];
  // Timing rows keep latency, speed and request duration in one compact group.
  // 性能分组集中展示延迟、速度和请求耗时。
  const timingRows: Array<[string, string | null]> = [
    [t("conversation.usage.averageFirstToken"), first],
    [t("conversation.usage.averageSpeed"), speed],
    [t("conversation.usage.totalDuration"), bucket.requestDurationMs.samples > 0 && bucket.requestDurationMs.sum > 0 ? formatDurationSeconds(bucket.requestDurationMs.sum) : null],
    [t("conversation.usage.averageDuration"), bucket.requestDurationMs.samples > 0 && bucket.requestDurationMs.sum > 0 ? formatDurationSeconds(bucket.requestDurationMs.sum / bucket.requestDurationMs.samples) : null],
  ];
  // The persistent heading preserves unknown and partially reported totals while collapsed.
  // 常驻标题在收起时仍保留未知和部分上报累计的明确标记。
  const total = lifetimeTotal(bucket.totalTokens, bucket.requestCount);
  return <details className="vk-lifetime" aria-label={props.title}>
    <summary className="vk-lifetime-head">
      <strong>{props.title}</strong>
      <span className="vk-lifetime-total">
        <strong>{total}</strong><span>{tokenUnitLabel(locale)}</span>
        <ChevronDown aria-hidden="true" size={12} />
      </span>
    </summary>
    <div className="vk-lifetime-body">
      <LifetimeMetricRows title={t("conversation.usage.calls")} rows={callRows} />
      <LifetimeMetricRows title={t("conversation.usage.tokens")} rows={tokenRows} />
      <LifetimeMetricRows title={t("conversation.usage.timing")} rows={timingRows} />
      {bucket.unknownUsageCount > 0 ? <p className="vk-stats-note">{formatMessage(t("conversation.usage.requestsDidNotReport"), { unknownUsageCount: bucket.unknownUsageCount })}</p> : null}
    </div>
  </details>;
}

/** Renders only populated metric pairs; null values never create empty grid cells.
 * 仅渲染有内容的指标对，空值不会生成空白网格单元。
 * Title names the group; rows supply translated labels and optional values. Returns no node when all values are absent.
 * title 为分组名称；rows 提供翻译文案及可选值。全部缺失时不返回节点。 */
function LifetimeMetricRows(props: { title: string; rows: Array<[string, string | null]> }) {
  // Filter before layout so hidden dimensions cannot leave holes in the grid.
  // 在布局前过滤，避免隐藏维度留下网格空洞。
  const rows = props.rows.filter((row): row is [string, string] => row[1] !== null);
  if (rows.length === 0) return null;
  return <section className="vk-lifetime-group" aria-label={props.title}>
    <strong>{props.title}</strong>
    <dl>{rows.map(([label, value]) => (
      <div key={label}><dt>{label}</dt><dd>{value}</dd></div>
    ))}</dl>
  </section>;
}

/** Shows separate main and descendant totals in the context usage popover.
 * 在上下文用量弹层分别展示主会话与后代累计。
 * Props contain the resident aggregate or its availability error; returns the summary page.
 * 属性包含常驻累计或不可用原因；返回统计页面。 */
export function SessionLifetimeUsagePanel(props: { usage?: SessionLifetimeUsage | null; error?: string | null }) {
  const { t } = useI18n();
  if (!props.usage) return null;
  // Only actual usage opens a section; a reserved child with no requests is not a zero-filled report.
  // 仅实际用量开启分区；尚无请求的预留子代理不会形成全零报告。
  const usage = props.usage;
  const childCount = Object.keys(usage.childrenBySessionId).length;
  const hasMain = usage.main.requestCount > 0 || usage.main.toolCallCount > 0;
  const hasChildren = childCount > 0 && (usage.childrenTotal.requestCount > 0 || usage.childrenTotal.toolCallCount > 0);
  if (!hasMain && !hasChildren) return null;
  return <>
    {hasMain ? <LifetimeUsageBucket title={t("conversation.usage.mainSessionUsage")} bucket={usage.main} /> : null}
    {hasChildren ? <LifetimeUsageBucket title={formatMessage(t("conversation.usage.subagentUsageAgents"), { childCount })} bucket={usage.childrenTotal} /> : null}
  </>;
}

/** Floating child-owned usage chip; hover and keyboard focus reveal details.
 * 子会话自身的浮动用量胶囊；悬停与键盘焦点显示详情。
 * Props contain this child's bucket only; returns an accessible floating control.
 * 属性仅包含当前子会话统计桶；返回可访问的浮动控件。 */
export function ChildLifetimeUsageChip(props: { bucket?: SessionUsageBucket }) {
  const { t } = useI18n();
  const label = t("conversation.usage.childLifetimeUsage");
  if (!props.bucket || (props.bucket.requestCount === 0 && props.bucket.toolCallCount === 0)) return null;
  return <div className="vk-lifetime-chip-slot">
    <TooltipProvider><Tooltip><TooltipTrigger asChild>
      <button type="button" className="vk-lifetime-chip" aria-label={label}>
        {label} · {props.bucket ? lifetimeTotal(props.bucket.totalTokens, props.bucket.requestCount) : "—"}
      </button>
    </TooltipTrigger><TooltipContent side="bottom" align="end" className="vk-stats">
      <LifetimeUsageBucket title={label} bucket={props.bucket} />
      <p className="vk-stats-note">{t("conversation.usage.lifetimeReferenceTotalsAcross")}</p>
    </TooltipContent></Tooltip></TooltipProvider>
  </div>;
}
