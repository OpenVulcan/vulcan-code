import type { ReactNode } from "react";

import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { formatDurationSeconds } from "@/lib/usage-format";
import type { StatBucket } from "@/service/conversation-event-window-store";
import type { ConversationTurnStatsTranscriptNode } from "@/service/conversation-event-display-projection";
import type { UsageCoverage } from "@/service/conversation-usage-coverage";
import { averageTurnOutputSpeed, usageCacheRatio, usageMetricDisplay } from "./conversation-usage-display";

/** Props for one V1-native terminal Turn statistics card.
 * 一张 V1 原生终态 Turn 统计卡的属性。 */
export interface ConversationV1TurnStatsCardProps {
  /** Exact persisted totals plus optional known-usage evidence derived from API events, never transcript text.
   * 精确持久化总量及从 API 事件派生的可选已知用量证据，绝不从转录文本重算。 */
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
  // event owns exact totals; view-only coverage may additionally expose proven partial contributions.
  // event 持有精确总量；仅供视图的覆盖信息可额外展示已证明的部分贡献。
  const event = props.node.item.event;
  // total uses this session's own bucket, including when legacy history retains child totals.
  // total 使用当前会话自身统计桶，包含仍保留子代理总量的旧历史。
  const total = event.buckets.direct;
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
    : formatDurationSeconds(event.duration_ms);
  // Both display surfaces use the same measured request cohort, excluding child aggregates without timing.
  // 两条显示路径使用同一已测请求样本组，不含缺少计时的子代理聚合。
  const timing = props.node.usage_coverage?.outputTiming;
  const speed = averageTurnOutputSpeed(timing?.outputTokens, timing?.outputDurationMs);
  // Tiny positive measurements remain distinct from a measured zero.
  // 微小正数测量仍与测得零值区分。
  const speedLabel = speed === null ? "-" : `${speed > 0 && speed < 0.1 ? "<0.1" : speed.toFixed(1)} tokens/s`;
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
            <span title={speed === null ? (zh ? "缺少配对的首 Token 时间与输出用量" : "Paired first-token timing and output usage are unavailable") : (zh ? `已测 ${timing?.requestCount} 次请求：输出 Token ÷ 首 Token 后累计输出时长，不含请求之间的工具等待` : `${timing?.requestCount} measured requests: output tokens divided by summed output duration after first token, excluding inter-request tool waits`)}>{zh ? `平均 ${speedLabel}` : `Average ${speedLabel}`}</span>
          </div>
          <TurnStatsUsageRows bucket={total} coverage={props.node.usage_coverage?.direct} locale={locale}>
            {timing !== undefined && timing.requestCount > 0 ? <>
              <TurnStatsMetricRow
                leftLabel={zh ? "平均首 Token" : "Average first token"}
                leftValue={formatDurationSeconds(timing.firstTokenLatencyMs / timing.requestCount)}
                rightLabel={zh ? "累计输出时长" : "Total output time"}
                rightValue={formatDurationSeconds(timing.outputDurationMs)}
              />
              {durationLabel !== null ? <div className="turn-stats-metric-row">
                <div className="turn-stats-metric"><span>{zh ? "回合总时长" : "Turn duration"}</span><strong>{durationLabel}</strong></div>
              </div> : null}
            </> : null}
          </TurnStatsUsageRows>
          <TurnStatsCacheSummary bucket={total} coverage={props.node.usage_coverage?.direct} locale={locale} />
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

/** Renders paired accounting and optional timing rows using the existing typography.
 * 使用既有字体样式渲染双列核算数据及可选计时行。
 *
 * @param props persisted bucket, optional coverage and timing rows, and display locale.
 *              持久化统计桶、可选覆盖信息与计时行及展示语言。
 * @returns the aligned metric grid.
 *          对齐后的统计网格。 */
function TurnStatsUsageRows(props: {
  bucket: StatBucket;
  coverage?: UsageCoverage;
  locale: "zh-CN" | "en-US";
  // Optional timing rows use the same grid as accounting values.
  // 可选计时行与核算数据使用同一网格。
  children?: ReactNode;
}) {
  // zh selects compact labels while values remain untouched.
  // zh 选择紧凑标签，而数值保持不变。
  const zh = props.locale === "zh-CN";
  // Metrics retain their existing sources without adding visible coverage qualifiers.
  // 指标保持既有来源，不添加可见覆盖限定说明。
  const cached = usageMetricDisplay(props.bucket.usage.cache_read_tokens, "cache_read_tokens", props.coverage);
  // Uncached input never substitutes the complete Provider input count.
  // 无缓存输入绝不使用完整 Provider 输入数替代。
  const uncached = usageMetricDisplay(props.bucket.usage.uncached_input_tokens, "uncached_input_tokens", props.coverage);
  // Output uses the shared numeric format without changing source usage.
  // 输出使用共享数值格式，不改变源用量。
  const output = usageMetricDisplay(props.bucket.usage.output_tokens, "output_tokens", props.coverage);
  // Total follows the same plain numeric display policy as its component dimensions.
  // 总量与各分项使用相同的纯数值展示规则。
  const total = usageMetricDisplay(props.bucket.usage.total_tokens, "total_tokens", props.coverage);
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
        leftValue={cached}
        rightLabel={zh ? "无缓存输入" : "Uncached input"}
        rightValue={uncached}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "输出 Tokens" : "Output tokens"}
        leftValue={output}
        rightLabel={zh ? "合计 Tokens" : "Total tokens"}
        rightValue={total}
      />
      {props.children}
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

/** Renders the existing cache-hit ratio with the original short label and progress bar.
 * 使用原有简短标签和进度条渲染既有缓存命中率。
 *
 * @param props persisted bucket, optional paired coverage, and display locale.
 *              持久化统计桶、可选配对覆盖信息及展示语言。
 * @returns one cache summary with an indeterminate text state when facts are absent.
 *          一条缓存摘要；事实缺失时显示无数据状态。 */
function TurnStatsCacheSummary(props: { bucket: StatBucket; coverage?: UsageCoverage; locale: "zh-CN" | "en-US" }) {
  // zh selects the localized cache labels.
  // zh 选择本地化缓存标签。
  const zh = props.locale === "zh-CN";
  // ratio never combines input components belonging to different request cohorts.
  // ratio 绝不混合属于不同请求样本组的输入分项。
  const ratio = usageCacheRatio(props.bucket.usage.cache_read_tokens, props.bucket.usage.uncached_input_tokens, props.coverage);
  // Missing or empty paired evidence remains an indeterminate percentage.
  // 缺失或为空的配对证据保持百分比未知。
  const percentage = ratio.percent;
  // Keep internal coverage out of the compact card's visible labels.
  // 不在紧凑卡的可见标签中展示内部覆盖信息。
  const label = zh ? "缓存命中率" : "Cache hit rate";
  return (
    <div className="turn-stats-cache-summary">
      <div className="turn-stats-cache-header">
        <span>{label}</span>
        <strong title={percentage === null ? (zh ? "尚无完整输入用量，无法计算缓存命中率" : "Cache hit rate needs complete input usage") : undefined}>{percentage === null ? "-" : `${percentage.toFixed(1)}%`}</strong>
      </div>
      <div
        aria-label={label}
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
