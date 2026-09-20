import { useI18n } from "@/i18n";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import type { SessionLifetimeUsage, SessionUsageBucket, UsageMeasurement } from "@/types/session-lifetime-usage";

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
  if (!bucket) return <section><strong>{props.title}</strong><p>{zh ? "累计用量暂不可用" : "Lifetime usage unavailable"}</p></section>;
  // Averages use reporting samples and never treat missing timing as zero.
  // 均值使用已上报样本，不将缺失计时视为零。
  const first = bucket.firstTokenMs.samples > 0 ? `${(bucket.firstTokenMs.sum / bucket.firstTokenMs.samples / 1000).toFixed(3)} s` : "—";
  const speed = bucket.outputDurationMs.sum > 0 ? `${(bucket.measuredOutputTokens * 1000 / bucket.outputDurationMs.sum).toFixed(1)} t/s` : "—";
  const cache = bucket.cacheInputTokens.sum > 0 ? `${(bucket.measuredCacheReadTokens * 100 / bucket.cacheInputTokens.sum).toFixed(1)}%` : "—";
  const rows = [
    [zh ? "API 请求" : "API requests", bucket.requestCount.toLocaleString()],
    [zh ? "工具调用" : "Tool calls", bucket.toolCallCount.toLocaleString()],
    [zh ? "总 Token" : "Total tokens", lifetimeTotal(bucket.totalTokens, bucket.requestCount)],
    [zh ? "输入" : "Input", lifetimeTotal(bucket.inputTokens, bucket.requestCount)],
    [zh ? "未缓存输入" : "Uncached input", lifetimeTotal(bucket.uncachedInputTokens, bucket.requestCount)],
    [zh ? "输出" : "Output", lifetimeTotal(bucket.outputTokens, bucket.requestCount)],
    [zh ? "推理" : "Reasoning", lifetimeTotal(bucket.reasoningTokens, bucket.requestCount)],
    [zh ? "缓存读取" : "Cache read", lifetimeTotal(bucket.cacheReadTokens, bucket.requestCount)],
    [zh ? "缓存写入" : "Cache write", lifetimeTotal(bucket.cacheWriteTokens, bucket.requestCount)],
    [zh ? "平均首 Token" : "Average first token", first],
    [zh ? "平均输出速度" : "Average output speed", speed],
    [zh ? "累计请求时长" : "Summed request time", bucket.requestDurationMs.samples ? `${(bucket.requestDurationMs.sum / 1000).toFixed(1)} s` : "—"],
    [zh ? "平均请求时长" : "Average request time", bucket.requestDurationMs.samples ? `${(bucket.requestDurationMs.sum / bucket.requestDurationMs.samples / 1000).toFixed(3)} s` : "—"],
    [zh ? "缓存命中率" : "Cache hit rate", cache],
  ];
  return <section className="space-y-2" aria-label={props.title}>
    <strong>{props.title}</strong>
    <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
      {rows.map(([label, value]) => <div key={label}><dt className="text-muted-foreground">{label}</dt><dd className="font-medium tabular-nums">{value}</dd></div>)}
    </dl>
    <p className="text-xs text-muted-foreground">{zh
      ? `有效样本：首 Token ${bucket.firstTokenMs.samples} 次 · 速度 ${bucket.outputDurationMs.samples} 次 · 请求时长 ${bucket.requestDurationMs.samples} 次 · 缓存率 ${bucket.cacheInputTokens.samples} 次`
      : `Samples: first token ${bucket.firstTokenMs.samples} · speed ${bucket.outputDurationMs.samples} · request time ${bucket.requestDurationMs.samples} · cache ${bucket.cacheInputTokens.samples}`}</p>
    {bucket.unknownUsageCount > 0 ? <p className="text-xs text-muted-foreground">{zh ? `有 ${bucket.unknownUsageCount} 次请求未报告用量` : `${bucket.unknownUsageCount} requests did not report usage`}</p> : null}
  </section>;
}

/** Shows separate main and descendant totals in the context usage popover.
 * 在上下文用量弹层分别展示主会话与后代累计。
 * Props contain the resident aggregate or its availability error; returns the summary page.
 * 属性包含常驻累计或不可用原因；返回统计页面。 */
export function SessionLifetimeUsagePanel(props: { usage?: SessionLifetimeUsage | null; error?: string | null }) {
  const { locale } = useI18n();
  const zh = locale === "zh-CN";
  if (!props.usage) return <p className="text-xs text-muted-foreground" title={props.error ?? undefined}>{zh ? "会话累计暂不可用" : "Lifetime usage unavailable"}</p>;
  return <div className="max-h-96 space-y-4 overflow-auto border-t pt-3">
    <LifetimeUsageBucket title={zh ? "主会话自身累计" : "Main session usage"} bucket={props.usage.main} />
    <LifetimeUsageBucket title={zh ? `子代理累计 · ${Object.keys(props.usage.childrenBySessionId).length} 个` : `Subagent usage · ${Object.keys(props.usage.childrenBySessionId).length} agents`} bucket={props.usage.childrenTotal} />
    <p className="text-xs text-muted-foreground">{zh ? "自创建起累计，仅供参考；不同模型的速度合并计算。" : "Lifetime reference totals; speed combines different models."}
      {props.usage.legacyIncomplete ? (zh ? " 部分旧历史无法补建。" : " Some legacy history is unavailable.") : ""}</p>
  </div>;
}

/** Floating child-owned usage chip; hover and keyboard focus reveal details.
 * 子会话自身的浮动用量胶囊；悬停与键盘焦点显示详情。
 * Props contain this child's bucket only; returns an accessible floating control.
 * 属性仅包含当前子会话统计桶；返回可访问的浮动控件。 */
export function ChildLifetimeUsageChip(props: { bucket?: SessionUsageBucket }) {
  const { locale } = useI18n();
  const label = locale === "zh-CN" ? "子会话累计用量" : "Child lifetime usage";
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
