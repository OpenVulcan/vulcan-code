import { useI18n } from "@/i18n";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { SessionLifetimeUsage, SessionUsageBucket, UsageMeasurement } from "@/types/session-lifetime-usage";
import "./session-lifetime-usage.css";

/** Formats a measured total, preserving unknown and partial reporting.
 * 格式化实测累计，保留未知与部分上报。
 * Returns a display string for one measurement and expected request count.
 * 返回一项测量与预期请求数对应的显示文字。 */
export function lifetimeTotal(value: UsageMeasurement, requests: number): string {
  if (requests === 0) return "0";
  if (value.samples === 0) return "—";
  return `${value.samples < requests ? "≥ " : ""}${value.sum.toLocaleString()}`;
}

/** Renders one own-session aggregate with averages derived from matched samples.
 * 展示一份会话自身累计，均值由匹配样本计算。
 * Props contain a title and an optional measured bucket; returns a metric section.
 * 属性包含标题与可选实测桶；返回统计区块。 */
export function LifetimeUsageBucket(props: { title: string; bucket?: SessionUsageBucket }) {
  // Locale controls labels only; the backend owns every counter.
  // 语言仅控制文案；所有计数由后端持有。
  const { locale } = useI18n();
  const zh = locale === "zh-CN";
  const bucket = props.bucket;
  if (!bucket || (bucket.requestCount === 0 && bucket.toolCallCount === 0)) return null;
  // Averages use reporting samples and never treat missing timing as zero.
  // 均值使用已上报样本，不将缺失计时视为零。
  const first = bucket.firstTokenMs.samples > 0 && bucket.firstTokenMs.sum > 0 ? `${(bucket.firstTokenMs.sum / bucket.firstTokenMs.samples / 1000).toFixed(3)} s` : null;
  const speed = bucket.outputDurationMs.samples > 0 && bucket.outputDurationMs.sum > 0 && bucket.measuredOutputTokens > 0 ? `${(bucket.measuredOutputTokens * 1000 / bucket.outputDurationMs.sum).toFixed(1)} t/s` : null;
  // Keep partial totals explicit, but omit zero and unreported token dimensions from the visible layout.
  // 保留部分上报累计的明确标记，但不让零值及未上报维度占据界面。
  const tokenValue = (value: UsageMeasurement): string | null => value.samples > 0 && value.sum > 0
    ? lifetimeTotal(value, bucket.requestCount) : null;
  const primaryRows: Array<[string, string | null]> = [
    [zh ? "输入" : "Input", tokenValue(bucket.inputTokens)],
    [zh ? "输出" : "Output", tokenValue(bucket.outputTokens)],
    [zh ? "平均首 Token" : "Average first token", first],
    [zh ? "平均速度" : "Average speed", speed],
  ];
  // Secondary accounting stays available on demand without competing with context capacity.
  // 次要核算信息按需展开，不与上下文容量争抢默认空间。
  const detailRows: Array<[string, string | null]> = [
    [zh ? "未缓存输入" : "Uncached input", tokenValue(bucket.uncachedInputTokens)],
    [zh ? "缓存读取" : "Cache read", tokenValue(bucket.cacheReadTokens)],
    [zh ? "缓存写入" : "Cache write", tokenValue(bucket.cacheWriteTokens)],
    [zh ? "推理" : "Reasoning", tokenValue(bucket.reasoningTokens)],
    [zh ? "累计请求时长" : "Summed request time", bucket.requestDurationMs.samples > 0 && bucket.requestDurationMs.sum > 0 ? `${(bucket.requestDurationMs.sum / 1000).toFixed(1)} s` : null],
    [zh ? "平均请求时长" : "Average request time", bucket.requestDurationMs.samples > 0 && bucket.requestDurationMs.sum > 0 ? `${(bucket.requestDurationMs.sum / bucket.requestDurationMs.samples / 1000).toFixed(3)} s` : null],
    [zh ? "缓存命中率" : "Cache hit rate", bucket.cacheInputTokens.samples > 0 && bucket.cacheInputTokens.sum > 0 ? `${(bucket.measuredCacheReadTokens * 100 / bucket.cacheInputTokens.sum).toFixed(1)}%` : null],
  ];
  const total = tokenValue(bucket.totalTokens);
  return <section className="lifetime-usage-section" aria-label={props.title}>
    <header className="lifetime-usage-heading">
      <strong>{props.title}</strong>
      <span>{[
        bucket.requestCount > 0 ? (zh ? `${bucket.requestCount.toLocaleString()} 次请求` : `${bucket.requestCount.toLocaleString()} requests`) : null,
        bucket.toolCallCount > 0 ? (zh ? `${bucket.toolCallCount.toLocaleString()} 次工具调用` : `${bucket.toolCallCount.toLocaleString()} tool calls`) : null,
      ].filter(Boolean).join(" · ")}</span>
    </header>
    {total ? <div className="lifetime-usage-total"><strong>{total}</strong><span>tokens</span></div> : null}
    <LifetimeMetricRows rows={primaryRows} />
    {detailRows.some(([, value]) => value !== null) ? <details className="lifetime-usage-details">
      <summary>{zh ? "更多统计" : "More statistics"}</summary>
      <LifetimeMetricRows rows={detailRows} />
    </details> : null}
    {bucket.unknownUsageCount > 0 ? <p className="lifetime-usage-note">{zh ? `${bucket.unknownUsageCount} 次请求未上报用量，累计值不完整` : `${bucket.unknownUsageCount} requests did not report usage; totals are incomplete`}</p> : null}
  </section>;
}

/** Renders only populated metric pairs; null values never create empty grid cells.
 * 仅渲染有内容的指标对，空值不会生成空白网格单元。
 * Rows supply translated labels and optional formatted values; returns no node when all values are absent.
 * 行数据提供翻译文案及可选格式化值，全部缺失时不返回节点。 */
function LifetimeMetricRows(props: { rows: Array<[string, string | null]> }) {
  // Filter before layout so hidden dimensions cannot leave holes in the grid.
  // 在布局前过滤，避免隐藏维度留下网格空洞。
  const rows = props.rows.filter((row): row is [string, string] => row[1] !== null);
  if (rows.length === 0) return null;
  return <dl className="lifetime-usage-metrics">{rows.map(([label, value]) => (
    <div key={label}><dt>{label}</dt><dd>{value}</dd></div>
  ))}</dl>;
}

/** Shows separate main and descendant totals in the context usage popover.
 * 在上下文用量弹层分别展示主会话与后代累计。
 * Props contain the resident aggregate or its availability error; returns the summary page.
 * 属性包含常驻累计或不可用原因；返回统计页面。 */
export function SessionLifetimeUsagePanel(props: { usage?: SessionLifetimeUsage | null; error?: string | null }) {
  const { locale } = useI18n();
  const zh = locale === "zh-CN";
  if (!props.usage) return null;
  // Only actual usage opens a section; a reserved child with no requests is not a zero-filled report.
  // 仅实际用量开启分区；尚无请求的预留子代理不会形成全零报告。
  const usage = props.usage;
  const childCount = Object.keys(usage.childrenBySessionId).length;
  const hasMain = usage.main.requestCount > 0 || usage.main.toolCallCount > 0;
  const hasChildren = childCount > 0 && (usage.childrenTotal.requestCount > 0 || usage.childrenTotal.toolCallCount > 0);
  if (!hasMain && !hasChildren) return null;
  return <div className="session-lifetime-usage">
    {hasMain ? <LifetimeUsageBucket title={zh ? "会话累计" : "Main session usage"} bucket={usage.main} /> : null}
    {hasChildren ? <LifetimeUsageBucket title={zh ? `子代理累计 · ${childCount} 个` : `Subagent usage · ${childCount} agents`} bucket={usage.childrenTotal} /> : null}
    {usage.legacyIncomplete ? <p className="lifetime-usage-note">{zh ? "部分历史用量缺失，累计值不完整。" : "Some history is unavailable; totals are incomplete."}</p> : null}
  </div>;
}

/** Floating child-owned usage chip; hover and keyboard focus reveal details.
 * 子会话自身的浮动用量胶囊；悬停与键盘焦点显示详情。
 * Props contain this child's bucket only; returns an accessible floating control.
 * 属性仅包含当前子会话统计桶；返回可访问的浮动控件。 */
export function ChildLifetimeUsageChip(props: { bucket?: SessionUsageBucket }) {
  const { locale } = useI18n();
  const label = locale === "zh-CN" ? "子会话累计用量" : "Child lifetime usage";
  if (!props.bucket || (props.bucket.requestCount === 0 && props.bucket.toolCallCount === 0)) return null;
  return <div className="pointer-events-none absolute right-4 top-2 z-20">
    <TooltipProvider><Tooltip><TooltipTrigger asChild>
      <button type="button" className="pointer-events-auto rounded-full border bg-background/95 px-3 py-1 text-xs shadow-sm" aria-label={label}>
        {label} · {props.bucket ? lifetimeTotal(props.bucket.totalTokens, props.bucket.requestCount) : "—"}
      </button>
    </TooltipTrigger><TooltipContent side="bottom" align="end" className="context-usage-tooltip pointer-events-auto">
      <LifetimeUsageBucket title={label} bucket={props.bucket} />
      <p className="mt-2 text-xs text-muted-foreground">{locale === "zh-CN" ? "自创建起累计，仅供参考；不同模型合并计算。" : "Lifetime reference totals across models."}</p>
    </TooltipContent></Tooltip></TooltipProvider>
  </div>;
}
