import * as React from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { loadOpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { formatCount, formatMicroPoints } from "./openvulcan-usage-format";
import { readUsageStats, shareOf, type UsageCounters, type UsageStats } from "./openvulcan-usage-stats";

// TrendMetric chooses which counter the daily bars measure.
// TrendMetric 选择每日柱条衡量的计数。
type TrendMetric = "requests" | "points";

// metricValue returns the counter a trend metric measures.
// metricValue 返回趋势指标所衡量的计数。
function metricValue(counters: UsageCounters, metric: TrendMetric): bigint {
  return metric === "requests" ? counters.requests : counters.microPoints;
}

// formatDay shows an ISO date as a short UTC month/day for locale, or the raw text when it is not a date.
// formatDay 将 ISO 日期按 locale 显示为简短的 UTC 月/日；不是日期时原样返回。
function formatDay(date: string, locale: string): string {
  const time = Date.parse(`${date}T00:00:00Z`);
  if (Number.isNaN(time)) return date;
  return new Intl.DateTimeFormat(locale, { month: "2-digit", day: "2-digit", timeZone: "UTC" }).format(time);
}

// OpenVulcanUsageStatsPanel shows folded usage statistics: totals, a daily trend and the per-model split.
// Hourly records stay on the account page's call details tab.
// OpenVulcanUsageStatsPanel 显示折叠后的用量统计：合计、每日趋势与按模型分布；
// 小时明细保留在账户页的调用详情标签中。
// Parameters: totalsOnly keeps just the totals for the quick dialog; the trend and model split stay on the page.
// 参数：totalsOnly 仅保留合计，用于快捷弹窗；趋势与按模型分布保留在账户页。
// The parent keys this component by connection ID so a replaced account never shows stale statistics.
// 父组件按连接 ID 设置本组件 key，使替换后的账户不会显示过期统计。
export function OpenVulcanUsageStatsPanel(props: { totalsOnly?: boolean }) {
  // t and locale own copy and number formatting; stats, busy and error own the latest read.
  // t 与 locale 负责文案与数字格式；stats、busy 与 error 持有最近一次读取。
  const { locale, t } = useI18n();
  const [stats, setStats] = React.useState<UsageStats | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [metric, setMetric] = React.useState<TrendMetric>("requests");
  // version invalidates reads on unmount or a newer refresh.
  // version 在卸载或更新的刷新时使读取失效。
  const version = React.useRef(0);
  const read = React.useCallback(async () => {
    const generation = ++version.current;
    setBusy(true);
    setError(null);
    try {
      const next = await readUsageStats(loadOpenVulcanUsagePage);
      if (generation === version.current) setStats(next);
    } catch (cause) { if (generation === version.current) setError(errorMessage(cause)); }
    finally { if (generation === version.current) setBusy(false); }
  }, []);
  React.useEffect(() => {
    void read();
    return () => { version.current += 1; };
  }, [read]);

  const totals = stats?.totals;
  const peak = stats ? stats.days.reduce((max, day) => {
    const value = metricValue(day.counters, metric);
    return value > max ? value : max;
  }, 0n) : 0n;
  // Model shares measure charges when any model was charged (models sort by charge first), otherwise requests.
  // 只要有模型产生扣费（模型按扣费优先排序），模型占比按扣费衡量，否则按请求次数衡量。
  const modelMetric = stats?.models[0] ? (stats.models[0].counters.microPoints > 0n ? "points" : "requests") : "requests";
  const modelWhole = totals ? metricValue(totals, modelMetric) : 0n;
  const metricText = (counters: UsageCounters) => metric === "requests"
    ? formatCount(counters.requests.toString(), locale)
    : formatMicroPoints(counters.microPoints.toString(), locale);

  return <section className="openvulcan-usage openvulcan-stats" aria-busy={busy}>
    <header className="openvulcan-usage-header">
      <h3>{t("openvulcan.stats.title")}</h3>
      {stats ? <span>{formatMessage(t("openvulcan.usage.range"), { from: stats.fromDate, to: stats.toDate })}</span> : null}
      <Button variant="ghost" size="sm" disabled={busy} data-openvulcan-stats-refresh="" onClick={() => void read()}>
        {busy ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <RefreshCw aria-hidden="true" />}
        {t("openvulcan.usage.refresh")}
      </Button>
    </header>
    {error ? <SettingsErrorNotice message={t("openvulcan.account.failed")} detail={error} /> : null}
    {totals ? <dl className="openvulcan-usage-totals" data-count="4">
      <div>
        <dt>{t("openvulcan.usage.requests")}</dt>
        <dd>{formatCount(totals.requests.toString(), locale)}</dd>
        {totals.failed > 0n ? <small data-tone="bad">{formatMessage(t("openvulcan.usage.failed"), { count: formatCount(totals.failed.toString(), locale) })}</small> : null}
      </div>
      <div><dt>{t("openvulcan.usage.tokens")}</dt><dd>{formatCount(totals.tokens.toString(), locale)}</dd></div>
      <div><dt>{t("openvulcan.stats.spent")}</dt><dd>{formatMicroPoints(totals.microPoints.toString(), locale)}</dd></div>
      <div><dt>{t("openvulcan.stats.models")}</dt><dd>{formatCount(String(stats.models.length), locale)}</dd></div>
    </dl> : null}
    {stats?.partial ? <p className="openvulcan-stats-note">{formatMessage(t("openvulcan.stats.partial"), { from: stats.days.find((day) => day.counters.requests > 0n)?.date ?? stats.fromDate })}</p> : null}
    {stats && stats.models.length === 0 ? <PageEmptyState title={t("openvulcan.usage.empty")} /> : null}
    {stats && stats.models.length > 0 && !props.totalsOnly ? <>
      <div className="openvulcan-stats-block">
        <div className="openvulcan-stats-block-head">
          <h4>{t("openvulcan.stats.daily")}</h4>
          <div className="openvulcan-stats-metric" role="group" aria-label={t("openvulcan.stats.metric")}>
            {(["requests", "points"] as const).map((value) => (
              <button aria-pressed={metric === value} key={value} onClick={() => setMetric(value)} type="button">
                {t(value === "requests" ? "openvulcan.usage.requests" : "openvulcan.usage.points")}
              </button>
            ))}
          </div>
        </div>
        <ol className="openvulcan-stats-bars" aria-label={t("openvulcan.stats.daily")}>
          {stats.days.map((day) => {
            const value = metricValue(day.counters, metric);
            const label = `${formatDay(day.date, locale)} · ${metricText(day.counters)}`;
            return <li key={day.date} title={label} aria-label={label}>
              <i data-empty={value === 0n || undefined} style={{ height: `${Math.max(shareOf(value, peak), value > 0n ? 4 : 0)}%` }} />
            </li>;
          })}
        </ol>
        <div className="openvulcan-stats-axis">
          <span>{formatDay(stats.fromDate, locale)}</span>
          <span>{formatDay(stats.toDate, locale)}</span>
        </div>
      </div>
      <div className="openvulcan-stats-block">
        <div className="openvulcan-stats-block-head"><h4>{t("openvulcan.stats.byModel")}</h4></div>
        <div className="openvulcan-stats-models" role="table" aria-label={t("openvulcan.stats.byModel")}>
          <div role="row" className="openvulcan-stats-model-row" data-head="">
            <span role="columnheader">{t("openvulcan.usage.model")}</span>
            <span role="columnheader">{t("openvulcan.usage.requests")}</span>
            <span role="columnheader">{t("openvulcan.usage.tokens")}</span>
            <span role="columnheader">{t("openvulcan.usage.points")}</span>
          </div>
          {stats.models.map(({ modelId, counters }) => (
            <div role="row" className="openvulcan-stats-model-row" key={modelId}>
              <span role="cell" className="openvulcan-stats-model-name">
                <span title={modelId}>{modelId}</span>
                <span className="openvulcan-plan-meter" aria-hidden="true">
                  <i style={{ width: `${shareOf(metricValue(counters, modelMetric), modelWhole)}%` }} />
                </span>
              </span>
              <span role="cell">
                {formatCount(counters.requests.toString(), locale)}
                {counters.failed > 0n ? <small>{formatMessage(t("openvulcan.usage.failed"), { count: formatCount(counters.failed.toString(), locale) })}</small> : null}
              </span>
              <span role="cell">{formatCount(counters.tokens.toString(), locale)}</span>
              <span role="cell">{formatMicroPoints(counters.microPoints.toString(), locale)}</span>
            </div>
          ))}
        </div>
      </div>
    </> : null}
  </section>;
}
