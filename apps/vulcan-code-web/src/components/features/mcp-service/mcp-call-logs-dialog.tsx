import * as React from "react";
import { FileText, History, RefreshCw } from "lucide-react";
import { PageEmptyState, PageItem, PageItemList, PageNotice } from "@/components/page";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage, formatDateTime } from "@/lib/display-format";
import { loadMcpCallDetail, loadMcpCallLogs } from "@/service/mcp-service-service";
import type { McpCallDetail, McpCallLogPage, McpCallLogQuery, McpCallLogRow, McpServiceClientSummary } from "@/types";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";

/// Shows local MCP call history, requesting full contents only when a row is opened.
/// 展示本地 MCP 调用历史，打开某条记录时才请求完整内容。
export function McpCallLogsDialog(props: {
  clients: McpServiceClientSummary[];
  clientId?: string;
  onClose: () => void;
}) {
  const { t, locale } = useI18n();
  // Keep the form draft separate from the applied query and its cursor.
  // 将筛选表单草稿与已应用查询及其游标分别保存。
  const [draft, setDraft] = React.useState<McpCallLogQuery>({ clientId: props.clientId, ascending: false });
  const [query, setQuery] = React.useState(draft);
  const [page, setPage] = React.useState<McpCallLogPage | null>(null);
  const [listError, setListError] = React.useState<string | null>(null);
  // Load details on demand so opening the history never fetches every payload.
  // 按需加载详情，打开历史时无需下载所有载荷。
  const [current, setCurrent] = React.useState<McpCallLogRow | null>(null);
  const [detail, setDetail] = React.useState<McpCallDetail | null>(null);
  const [detailError, setDetailError] = React.useState<string | null>(null);
  const [previousCursors, setPreviousCursors] = React.useState<(number | undefined)[]>([]);
  const [refresh, setRefresh] = React.useState(0);

  React.useEffect(() => {
    let active = true;
    setPage(null);
    setListError(null);
    void loadMcpCallLogs(query).then((result) => {
      if (active) setPage(result);
    }, (error: unknown) => {
      if (active) setListError(errorMessage(error));
    });
    return () => { active = false; };
  }, [query, refresh]);

  React.useEffect(() => {
    if (!current) return;
    let active = true;
    setDetail(null);
    setDetailError(null);
    void loadMcpCallDetail(current.id).then((result) => {
      if (active) setDetail(result);
    }, (error: unknown) => {
      if (active) setDetailError(errorMessage(error));
    });
    return () => { active = false; };
  }, [current, refresh]);

  // Returns to the first page with only the selected filters.
  // 使用当前选定的筛选条件返回第一页。
  const search = (filters: McpCallLogQuery) => {
    setPreviousCursors([]);
    setQuery({
      clientId: filters.clientId,
      projectId: filters.projectId?.trim() || undefined,
      sid: filters.sid?.trim() || undefined,
      ascending: filters.ascending,
    });
  };
  // Uses the row's scope directly so the user does not have to copy its identifier.
  // 直接用记录的范围筛选，省去手动复制编号。
  const filterByRow = (row: McpCallLogRow, level: "project" | "session") => {
    const filters = { clientId: query.clientId, projectId: row.projectId ?? undefined, sid: level === "session" ? row.sid ?? undefined : undefined, ascending: query.ascending };
    setDraft(filters);
    search(filters);
  };
  // Seeks the next page without moving the current filter.
  // 保持筛选条件并定位到下一页。
  const next = () => {
    if (page?.nextCursor == null) return;
    setPreviousCursors((cursors) => [...cursors, query.cursor]);
    setQuery({ ...query, cursor: page.nextCursor });
  };
  // Restores the exact cursor used to fetch the previous page.
  // 恢复读取上一页时使用的准确游标。
  const previous = () => {
    setQuery({ ...query, cursor: previousCursors.at(-1) });
    setPreviousCursors((cursors) => cursors.slice(0, -1));
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
      <DialogContent scrollMode="fixed-footer" size="lg" data-mcp-call-logs="">
        <DialogHeader>
          <DialogTitle>{t(current ? "mcpService.logs.detail" : "mcpService.logs.title")}</DialogTitle>
          <DialogDescription>
            {current
              ? [current.tool, current.clientName, current.projectId, current.sid, formatDateTime(current.startedAt, locale)].filter(Boolean).join(" · ")
              : t("mcpService.logs.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="vk-dialog-body vk-mcp-logs-content">
          {current ? (
            detailError ? <PageNotice tone="error" details={detailError} detailsLabel={t("settings.errors.showErrorDetails")}>{t("mcpService.logs.error")}</PageNotice>
              : detail ? (
                <div className="vk-mcp-log-payloads">
                  <section className="vk-mcp-snippet"><h3>{t("mcpService.logs.arguments")}</h3><pre>{JSON.stringify(detail.arguments, null, 2)}</pre></section>
                  <section className="vk-mcp-snippet">
                    <h3>{t("mcpService.logs.result")}</h3>
                    {detail.result === null ? <p className="vk-mcp-note">{t("mcpService.logs.noResult")}</p> : <pre>{JSON.stringify(detail.result, null, 2)}</pre>}
                  </section>
                </div>
              ) : <StreamPlaceholder count={2} label={t("mcpService.loading")} shape="rows" />
          ) : (
            <>
              <form className="vk-mcp-log-filters" onSubmit={(event) => { event.preventDefault(); search(draft); }}>
                <div>
                  <span className="vk-label">{t("mcpService.logs.client")}</span>
                  <Select value={draft.clientId ?? "all"} onValueChange={(clientId) => setDraft({ ...draft, clientId: clientId === "all" ? undefined : clientId })}>
                    <SelectTrigger aria-label={t("mcpService.logs.client")}><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">{t("mcpService.logs.allClients")}</SelectItem>
                      {props.clients.map((client) => <SelectItem key={client.id} value={client.id}>{client.name}</SelectItem>)}
                    </SelectContent>
                  </Select>
                </div>
                <label>
                  <span className="vk-label">{t("mcpService.logs.project")}</span>
                  <Input data-mcp-log-project="" value={draft.projectId ?? ""} onChange={(event) => setDraft({ ...draft, projectId: event.target.value })} placeholder="p_1" />
                </label>
                <label>
                  <span className="vk-label">{t("mcpService.logs.session")}</span>
                  <Input data-mcp-log-session="" value={draft.sid ?? ""} onChange={(event) => setDraft({ ...draft, sid: event.target.value })} placeholder="s_1" />
                </label>
                <div>
                  <span className="vk-label">{t("mcpService.logs.order")}</span>
                  <Select value={draft.ascending ? "oldest" : "newest"} onValueChange={(value) => setDraft({ ...draft, ascending: value === "oldest" })}>
                    <SelectTrigger aria-label={t("mcpService.logs.order")}><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="newest">{t("mcpService.logs.newest")}</SelectItem>
                      <SelectItem value="oldest">{t("mcpService.logs.oldest")}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <Button type="submit" size="sm">{t("mcpService.logs.search")}</Button>
              </form>
              {listError ? <PageNotice details={listError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">{t("mcpService.logs.error")}</PageNotice>
                : !page ? <StreamPlaceholder count={3} label={t("mcpService.loading")} shape="rows" />
                : page.rows.length === 0 ? <PageEmptyState title={t("mcpService.logs.empty")} description={t("mcpService.logs.emptyDetail")} />
                : <PageItemList>
                  {page.rows.map((row) => (
                    <PageItem key={row.id} icon={<History />} title={row.tool}
                      description={<>
                        {[row.clientName, formatDateTime(row.startedAt, locale)].filter(Boolean).join(" · ")}
                        {row.projectId ? <Button type="button" variant="link" size="sm" title={t("mcpService.logs.project")} onClick={() => filterByRow(row, "project")}>{row.projectId}</Button> : null}
                        {row.sid ? <Button type="button" variant="link" size="sm" title={t("mcpService.logs.session")} onClick={() => filterByRow(row, "session")}>{row.sid}</Button> : null}
                      </>}
                      meta={<>
                        <Badge variant={row.status === "failed" ? "destructive" : "secondary"}>{t(`mcpService.logs.status.${row.status}`)}</Badge>
                        {row.finishedAt !== null ? <span>{formatMessage(t("mcpService.logs.elapsed"), { ms: Math.max(0, row.finishedAt - row.startedAt).toLocaleString(locale) })}</span> : null}
                      </>}
                      actions={<Button data-mcp-log-id={row.id} type="button" size="sm" variant="ghost" onClick={() => setCurrent(row)}><FileText size={14} />{t("mcpService.logs.detail")}</Button>} />
                  ))}
                </PageItemList>}
            </>
          )}
        </div>
        <DialogFooter>
          {current ? <Button type="button" size="sm" variant="outline" onClick={() => setCurrent(null)}>{t("mcpService.logs.back")}</Button> : <>
            <Button type="button" size="sm" variant="ghost" onClick={() => setRefresh((value) => value + 1)}><RefreshCw size={14} />{t("mcpService.logs.refresh")}</Button>
            <Button type="button" size="sm" variant="outline" disabled={previousCursors.length === 0} onClick={previous}>{t("mcpService.logs.previous")}</Button>
            <Button type="button" size="sm" variant="outline" disabled={page?.nextCursor == null} onClick={next}>{t("mcpService.logs.next")}</Button>
          </>}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
