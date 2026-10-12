import * as React from "react";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadOpenVulcanUsagePage, type OpenVulcanUsagePage } from "@/service/openvulcan-account-service";

// OpenVulcanUsageHistory reads explicitly requested pages for one mounted account and drops stale responses.
// OpenVulcanUsageHistory 为一个挂载账户读取明确请求的用量页，并丢弃过期响应。
// The parent keys this component by connection ID; returns an exact, bounded hourly usage table.
// 父组件按连接 ID 设置本组件 key；返回准确、有界的小时用量表。
export function OpenVulcanUsageHistory() {
  // t, page, busy and error own localized presentation and the current bounded page.
  // t、page、busy 与 error 持有本地化展示及当前有界页面。
  const { t } = useI18n();
  const [page, setPage] = React.useState<OpenVulcanUsagePage | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // version invalidates observations on account replacement or dialog unmount.
  // version 在账户替换或弹窗卸载时使观测失效。
  const version = React.useRef(0);
  const loading = React.useRef(false);
  React.useEffect(() => () => { version.current += 1; }, []);
  // read replaces one page without accumulating unbounded history in browser memory.
  // read 替换单页，避免在浏览器内存中无限累积历史。
  const read = async (cursor: string) => {
    if (loading.current) return;
    loading.current = true;
    const generation = ++version.current;
    setBusy(true);
    setError(null);
    try {
      const next = await loadOpenVulcanUsagePage(cursor);
      if (generation === version.current) setPage(next);
    } catch (cause) { if (generation === version.current) setError(errorMessage(cause)); }
    finally { if (generation === version.current) { loading.current = false; setBusy(false); } }
  };
  return <section className="grid min-w-0 gap-2">
    <Button variant="outline" disabled={busy} onClick={() => void read("")}>{t("openvulcan.account.history")}</Button>
    {error ? <SettingsErrorNotice message={t("openvulcan.account.failed")} detail={error} /> : null}
    {page ? <>
      <p className="text-xs text-muted-foreground">{page.from_date} – {page.to_date} · UTC</p>
      <div className="max-h-72 overflow-auto rounded-md border">
        <Table>
          <TableHeader><TableRow><TableHead>{t("openvulcan.usage.hour")}</TableHead><TableHead>{t("openvulcan.usage.model")}</TableHead><TableHead>{t("openvulcan.usage.requests")}</TableHead><TableHead>{t("openvulcan.usage.tokens")}</TableHead><TableHead>{t("openvulcan.usage.microPoints")}</TableHead></TableRow></TableHeader>
          <TableBody>{page.usage.map((row) => <TableRow key={`${row.hour_start}:${row.model_id}`}><TableCell>{row.hour_start}</TableCell><TableCell>{row.model_id}</TableCell><TableCell>{row.request_count}</TableCell><TableCell>{row.input_tokens}</TableCell><TableCell>{row.charged_micro_points}</TableCell></TableRow>)}</TableBody>
        </Table>
        {page.usage.length === 0 ? <p className="p-3 text-sm text-muted-foreground">{t("openvulcan.usage.empty")}</p> : null}
      </div>
      {page.next_cursor ? <Button variant="ghost" disabled={busy} onClick={() => void read(page.next_cursor)}>{t("openvulcan.usage.next")}</Button> : null}
    </> : null}
  </section>;
}
