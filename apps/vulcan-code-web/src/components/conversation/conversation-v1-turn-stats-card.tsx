import { Bot } from "lucide-react";

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import type { StatBucket } from "@/service/conversation-event-window-store";
import type { ConversationTurnStatsTranscriptNode } from "@/service/conversation-event-display-projection";

/** Props for one V1-native terminal Turn statistics card.
 * 一张 V1 原生终态 Turn 统计卡的属性。 */
export interface ConversationV1TurnStatsCardProps {
  /** Exact persisted statistics node rendered without recomputation from transcript text.
   * 不通过转录文本重算、直接渲染的精确持久化统计节点。 */
  node: ConversationTurnStatsTranscriptNode;
}

/** Renders the established bordered statistics chip and its structured detail card.
 * 渲染既有的带边框统计胶囊及其结构化详情卡。
 *
 * @param props exact persisted Turn statistics.
 *              精确持久化的 Turn 统计。
 * @returns one focusable statistics chip with a hover/focus detail card.
 *          一个可聚焦并带悬浮/焦点详情卡的统计胶囊。 */
export function ConversationV1TurnStatsCard(props: ConversationV1TurnStatsCardProps) {
  // locale selects only display labels and number formatting; facts remain the persisted values.
  // locale 仅选择展示标签和数字格式；事实仍保持持久化值。
  const { locale } = useI18n();
  // zh keeps the compact historical Chinese wording on the Chinese surface.
  // zh 在中文界面保持历史紧凑文案。
  const zh = locale === "zh-CN";
  // event is the sole source for the row and detail card.
  // event 是统计行和详情卡的唯一数据源。
  const event = props.node.item.event;
  // total is the authoritative inclusive bucket for this completed Turn.
  // total 是当前已完成 Turn 的权威总计桶。
  const total = event.buckets.total;
  // summary preserves the established compact API/tool count row.
  // summary 保留既有的紧凑 API/工具计数行。
  const summary = zh
    ? `回合统计 · API ${formatNumber(total.api_call_count, locale)} · 工具 ${formatNumber(total.tool_call_count, locale)}`
    : `Turn usage · API ${formatNumber(total.api_call_count, locale)} · Tools ${formatNumber(total.tool_call_count, locale)}`;
  // statusClass maps the explicit terminal outcome to the existing visual status families.
  // statusClass 将明确终态结果映射到既有视觉状态族。
  const statusClass = event.outcome === "completed" ? "completed" : event.outcome;
  // durationLabel is shown only when the persisted event contains an exact duration.
  // durationLabel 仅在持久化事件包含精确耗时时显示。
  const durationLabel = event.duration_ms === undefined
    ? null
    : formatDuration(event.duration_ms, locale);
  // statisticsRow is the always-visible bordered chip restored from the previous transcript surface.
  // statisticsRow 是从旧转录界面恢复的始终可见带边框胶囊。
  const statisticsRow = (
    <div
      aria-label={summary}
      className={`turn-stats-row ${statusClass}`}
      data-event-kind="turn_stats"
      data-seq={props.node.seq}
      tabIndex={0}
    >
      <span aria-hidden="true">{turnStatsIcon(event.outcome)}</span>
      <strong>{summary}</strong>
    </div>
  );

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>{statisticsRow}</TooltipTrigger>
        <TooltipContent
          align="start"
          className="context-usage-tooltip turn-stats-tooltip"
          showArrow={false}
          side="top"
          sideOffset={8}
        >
          <div className="context-usage-card-header turn-stats-card-header">
            <strong>{zh ? "本轮统计" : "Turn usage"}</strong>
            <span>{durationLabel ?? (zh ? "终态统计" : "Terminal totals")}</span>
          </div>
          <TurnStatsUsageRows bucket={total} locale={locale} />
          <TurnStatsCacheSummary bucket={total} locale={locale} />
          {bucketHasFacts(event.buckets.subagent) ? (
            <section className="turn-stats-subagent-section">
              <div className="turn-stats-section-title">
                <Bot aria-hidden="true" size={13} />
                <span>{zh ? "子代理统计" : "Subagent usage"}</span>
              </div>
              <TurnStatsUsageRows bucket={event.buckets.subagent} locale={locale} />
            </section>
          ) : null}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

/** Renders four exact paired accounting rows for one persisted statistics bucket.
 * 为一个持久化统计桶渲染四行精确的双列核算数据。
 *
 * @param props persisted bucket plus display locale.
 *              持久化统计桶与展示语言。
 * @returns the aligned metric grid.
 *          对齐后的统计网格。 */
function TurnStatsUsageRows(props: { bucket: StatBucket; locale: "zh-CN" | "en-US" }) {
  // zh selects compact labels while values remain untouched.
  // zh 选择紧凑标签，而数值保持不变。
  const zh = props.locale === "zh-CN";
  return (
    <div className="turn-stats-metric-grid">
      <TurnStatsMetricRow
        leftLabel={zh ? "总请求次数" : "Requests"}
        leftValue={formatNumber(props.bucket.api_call_count, props.locale)}
        rightLabel={zh ? "工具调用次数" : "Tool calls"}
        rightValue={formatNumber(props.bucket.tool_call_count, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "未知用量请求" : "Unknown usage"}
        leftValue={formatNumber(props.bucket.unknown_usage_api_call_count, props.locale)}
        rightLabel={zh ? "子代理返回" : "Subagent returns"}
        rightValue={formatNumber(props.bucket.subagent_return_count, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "缓存 Tokens" : "Cached tokens"}
        leftValue={formatOptionalNumber(props.bucket.usage.cache_read_tokens, props.locale)}
        rightLabel={zh ? "无缓存输入" : "Uncached input"}
        rightValue={formatOptionalNumber(props.bucket.usage.uncached_input_tokens, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "输出 Tokens" : "Output tokens"}
        leftValue={formatOptionalNumber(props.bucket.usage.output_tokens, props.locale)}
        rightLabel={zh ? "合计 Tokens" : "Total tokens"}
        rightValue={formatOptionalNumber(props.bucket.usage.total_tokens, props.locale)}
      />
    </div>
  );
}

/** Renders one aligned pair inside the statistics detail grid.
 * 在统计详情网格中渲染一组对齐双列。
 *
 * @param props labels and already-formatted exact values.
 *              标签与已经格式化的精确数值。
 * @returns one two-column metric row.
 *          一行双列统计项。 */
function TurnStatsMetricRow(props: {
  leftLabel: string;
  leftValue: string;
  rightLabel: string;
  rightValue: string;
}) {
  return (
    <div className="turn-stats-metric-row">
      <div className="turn-stats-metric">
        <span>{props.leftLabel}</span>
        <strong>{props.leftValue}</strong>
      </div>
      <div className="turn-stats-metric">
        <span>{props.rightLabel}</span>
        <strong>{props.rightValue}</strong>
      </div>
    </div>
  );
}

/** Renders the exact cache-hit ratio when both denominator dimensions are present.
 * 在分母两个维度都存在时渲染精确缓存命中率。
 *
 * @param props persisted statistics bucket plus display locale.
 *              持久化统计桶与展示语言。
 * @returns one cache summary with an indeterminate text state when facts are absent.
 *          一条缓存摘要；事实缺失时显示无数据状态。 */
function TurnStatsCacheSummary(props: { bucket: StatBucket; locale: "zh-CN" | "en-US" }) {
  // zh selects the localized cache labels.
  // zh 选择本地化缓存标签。
  const zh = props.locale === "zh-CN";
  // percentage remains null unless both cached and uncached dimensions are authoritative.
  // percentage 在缓存与无缓存两个维度都权威时才有值。
  const percentage = cacheHitPercent(props.bucket);
  return (
    <div className="turn-stats-cache-summary">
      <div className="turn-stats-cache-header">
        <span>{zh ? "缓存命中率" : "Cache hit rate"}</span>
        <strong>{percentage === null ? (zh ? "暂无数据" : "No data") : `${percentage.toFixed(1)}%`}</strong>
      </div>
      <div
        aria-label={zh ? "缓存命中率" : "Cache hit rate"}
        aria-valuemax={100}
        aria-valuemin={0}
        aria-valuenow={percentage ?? undefined}
        className="turn-stats-cache-track"
        role="progressbar"
      >
        <div className="turn-stats-cache-fill" style={{ width: `${percentage ?? 0}%` }} />
      </div>
    </div>
  );
}

/** Returns the established compact glyph for one explicit Turn outcome.
 * 返回一个明确 Turn 结果对应的既有紧凑图形。
 *
 * @param outcome persisted Turn outcome.
 *                持久化 Turn 结果。
 * @returns one display-only glyph.
 *          一个纯展示图形。 */
function turnStatsIcon(outcome: "completed" | "abandoned" | "stopped"): string {
  if (outcome === "abandoned") return "!";
  if (outcome === "stopped") return "■";
  return "◔";
}

/** Calculates a cache-hit percentage only from explicit cached and uncached counts.
 * 仅根据明确的缓存与无缓存计数计算缓存命中率。
 *
 * @param bucket persisted statistics bucket.
 *               持久化统计桶。
 * @returns a percentage or null when either source dimension is absent.
 *          百分比；任一来源维度缺失时返回 null。 */
function cacheHitPercent(bucket: StatBucket): number | null {
  const cached = bucket.usage.cache_read_tokens;
  const uncached = bucket.usage.uncached_input_tokens;
  if (cached === undefined || uncached === undefined) return null;
  const totalInput = cached + uncached;
  return totalInput === 0 ? 0 : Math.min(100, Math.max(0, (cached / totalInput) * 100));
}

/** Reports whether one subagent bucket contains any persisted accounting fact.
 * 判断一个子代理统计桶是否包含任意持久化核算事实。
 *
 * @param bucket persisted subagent bucket.
 *               持久化子代理统计桶。
 * @returns true when a count or usage dimension is present.
 *          存在任意计数或用量维度时返回 true。 */
function bucketHasFacts(bucket: StatBucket): boolean {
  return bucket.api_call_count > 0
    || bucket.unknown_usage_api_call_count > 0
    || bucket.tool_call_count > 0
    || bucket.subagent_return_count > 0
    || Object.values(bucket.usage).some((value) => value !== undefined);
}

/** Formats one exact non-negative count in the active locale.
 * 使用当前语言格式化一个精确非负计数。
 *
 * @param value exact persisted count.
 *              精确持久化计数。
 * @param locale active display locale.
 *               当前展示语言。
 * @returns localized decimal text.
 *          本地化十进制文本。 */
function formatNumber(value: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(value);
}

/** Formats an optional exact count without turning absence into zero.
 * 格式化一个可选精确计数，同时不把缺失转换为零。
 *
 * @param value optional persisted count.
 *              可选持久化计数。
 * @param locale active display locale.
 *               当前展示语言。
 * @returns localized decimal text or an em dash for absent data.
 *          本地化十进制文本；数据缺失时返回破折号。 */
function formatOptionalNumber(value: number | undefined, locale: string): string {
  return value === undefined ? "—" : formatNumber(value, locale);
}

/** Formats an exact persisted duration without deriving throughput.
 * 格式化精确持久化耗时，同时不推导吞吐速度。
 *
 * @param durationMs exact persisted duration in milliseconds.
 *                   精确持久化毫秒耗时。
 * @param locale active display locale.
 *               当前展示语言。
 * @returns compact localized duration.
 *          紧凑本地化耗时。 */
function formatDuration(durationMs: number, locale: "zh-CN" | "en-US"): string {
  if (durationMs < 1000) return `${Math.max(0, Math.round(durationMs))} ms`;
  const seconds = durationMs / 1000;
  return locale === "zh-CN" ? `${seconds.toFixed(1)} 秒` : `${seconds.toFixed(1)} s`;
}
