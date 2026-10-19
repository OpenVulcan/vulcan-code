import * as React from "react";
import { ChevronRight, LoaderCircle, RefreshCw } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { formatMessage, useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { loadOpenVulcanUsagePage, type OpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { formatCount, formatMicroPoints, formatUsageHour, sumCounts } from "./openvulcan-usage-format";

// OpenVulcanUsageHistory reads the latest usage page when it mounts and pages back on request, dropping stale responses.
// OpenVulcanUsageHistory 挂载时读取最新用量页，按需向前翻页，并丢弃过期响应。
// The parent keys this component by connection ID; returns page totals and an hourly table.
// 父组件按连接 ID 设置本组件 key；返回本页合计与小时用量表。
export function OpenVulcanUsageHistory() {
  // t and locale own localized copy and number grouping; page, busy and error own the current bounded page.
  // t 与 locale 负责本地化文案与数字分组；page、busy 与 error 持有当前有界页面。
  const { locale, t } = useI18n();
  const [page, setPage] = React.useState<OpenVulcanUsagePage | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // version invalidates observations on account replacement, unmount or a newer read.
  // version 在账户替换、卸载或更新的读取时使观测失效。
  const version = React.useRef(0);
  // read replaces one page without accumulating unbounded history in browser memory.
  // read 替换单页，避免在浏览器内存中无限累积历史。
  const read = React.useCallback(async (nextCursor: string) => {
    const generation = ++version.current;
    setBusy(true);
    setError(null);
    try {
      const next = await loadOpenVulcanUsagePage(nextCursor);
      if (generation === version.current) setPage(next);
    } catch (cause) { if (generation === version.current) setError(errorMessage(cause)); }
    finally { if (generation === version.current) setBusy(false); }
  }, []);
  React.useEffect(() => {
    void read("");
    return () => { version.current += 1; };
  }, [read]);
  const totals = React.useMemo(() => page ? {
    requests: sumCounts(page.usage.map((row) => row.request_count)),
    failed: sumCounts(page.usage.map((row) => row.failed_requests)),
    tokens: sumCounts(page.usage.map((row) => row.input_tokens)),
    points: sumCounts(page.usage.map((row) => row.charged_micro_points)),
  } : null, [page]);
  return <section className="openvulcan-usage" aria-busy={busy}>
    <header className="openvulcan-usage-header">
      <h3>{t("openvulcan.usage.title")}</h3>
      {page ? <span>{formatMessage(t("openvulcan.usage.range"), { from: page.from_date, to: page.to_date })}</span> : null}
      <Button variant="ghost" size="sm" disabled={busy} data-openvulcan-usage-refresh="" onClick={() => void read("")}>
        {busy ? <LoaderCircle aria-hidden="true" className="vk-spin" /> : <RefreshCw aria-hidden="true" />}
        {t("openvulcan.usage.refresh")}
      </Button>
    </header>
    {error ? <SettingsErrorNotice message={t("openvulcan.account.failed")} detail={error} /> : null}
    {totals ? <dl className="openvulcan-usage-totals">
      <div>
        <dt>{t("openvulcan.usage.requests")}</dt>
        <dd>{formatCount(totals.requests, locale)}</dd>
        {totals.failed !== "0" ? <small>{formatMessage(t("openvulcan.usage.failed"), { count: formatCount(totals.failed, locale) })}</small> : null}
      </div>
      <div><dt>{t("openvulcan.usage.tokens")}</dt><dd>{formatCount(totals.tokens, locale)}</dd></div>
      <div><dt>{t("openvulcan.usage.points")}</dt><dd>{formatMicroPoints(totals.points, locale)}</dd></div>
    </dl> : null}
    {page && page.usage.length > 0 ? <div className="openvulcan-usage-table">
      <table>
        <thead><tr>
          <th scope="col">{t("openvulcan.usage.time")}</th>
          <th scope="col">{t("openvulcan.usage.model")}</th>
          <th scope="col">{t("openvulcan.usage.requests")}</th>
          <th scope="col">{t("openvulcan.usage.tokens")}</th>
          <th scope="col">{t("openvulcan.usage.points")}</th>
        </tr></thead>
        <tbody>{page.usage.map((row) => <tr key={`${row.hour_start}:${row.model_id}`}>
          <td>{formatUsageHour(row.hour_start, locale)}</td>
          <td className="openvulcan-usage-model">{row.model_id}</td>
          <td>
            {formatCount(row.request_count, locale)}
            {row.failed_requests !== "0" ? <small>{formatMessage(t("openvulcan.usage.failed"), { count: formatCount(row.failed_requests, locale) })}</small> : null}
          </td>
          <td>{formatCount(row.input_tokens, locale)}</td>
          <td>{formatMicroPoints(row.charged_micro_points, locale)}</td>
        </tr>)}</tbody>
      </table>
    </div> : page ? <PageEmptyState title={t("openvulcan.usage.empty")} /> : null}
    {page?.next_cursor ? <Button variant="ghost" size="sm" className="openvulcan-usage-next" disabled={busy} onClick={() => void read(page.next_cursor)}>
      {t("openvulcan.usage.next")}<ChevronRight aria-hidden="true" />
    </Button> : null}
  </section>;
}
