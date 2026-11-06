// Bounded, scrollable log of one command job. It opens on the newest lines and follows new output while the
// viewer stays at the bottom; scrolling to the top loads one earlier page at a time, and only a bounded number of
// rows is kept, so a long log never renders in full.
// 一个命令任务的有界可滚动日志。打开时显示最新的行，查看者停留在底部时跟随新输出；滚动到顶部时每次加载更早的一页，
// 且只保留有界数量的行，长日志永远不会整体渲染。
import * as React from "react";
import { useI18n } from "@/i18n";
import { PageEmptyState } from "@/components/page";
import { readShellJobOutput } from "@/service/session-activity-panel-service";
import type { ShellJobOutputRow } from "@/types";

// LOG_PAGE_LINES is the number of log lines one request asks for.
// LOG_PAGE_LINES 是单次请求的日志行数。
const LOG_PAGE_LINES = 200;

// LOG_RETAINED_ROWS bounds the rows kept in memory; rows beyond it are dropped from the far end.
// LOG_RETAINED_ROWS 限制内存中保留的行数；超出部分从远端丢弃。
const LOG_RETAINED_ROWS = 2000;

// LOG_FOLLOW_MS is how often a running job's log is read while shown.
// LOG_FOLLOW_MS 是显示期间读取运行中任务日志的间隔。
const LOG_FOLLOW_MS = 1500;

// LOG_EDGE_PX is the distance from an edge that counts as being at that edge.
// LOG_EDGE_PX 是视为处于边缘的距离。
const LOG_EDGE_PX = 24;

// ShellJobLogViewProps names the job.
// ShellJobLogViewProps 指明任务。
export interface ShellJobLogViewProps {
  // Root session.
  // 根会话。
  sessionId: string;
  // Job id.
  // 任务标识。
  jobId: string;
  // Whether the job still runs.
  // 任务是否仍在运行。
  running: boolean;
}

// ShellJobLogView renders the log.
// ShellJobLogView 渲染日志。
export function ShellJobLogView(props: ShellJobLogViewProps) {
  const { t } = useI18n();
  const [rows, setRows] = React.useState<ShellJobOutputRow[]>([]);
  const [hasEarlier, setHasEarlier] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [loaded, setLoaded] = React.useState(false);
  const scrollRef = React.useRef<HTMLDivElement | null>(null);
  const followRef = React.useRef(true);
  const loadingEarlierRef = React.useRef(false);
  const rowsRef = React.useRef<ShellJobOutputRow[]>([]);
  rowsRef.current = rows;
  // prependAnchor keeps the viewport still after earlier rows are inserted above it.
  // prependAnchor 在上方插入更早的行之后保持视口不动。
  const prependAnchorRef = React.useRef<number | null>(null);
  const { jobId, running, sessionId } = props;

  React.useEffect(() => {
    let cancelled = false;
    let timer: number | null = null;
    setRows([]);
    setLoaded(false);
    followRef.current = true;
    // follow re-reads from the last line shown, which may still have been growing, and replaces it.
    // follow 从已显示的最后一行重新读取（该行可能仍在增长）并替换它。
    const follow = async (): Promise<void> => {
      const current = rowsRef.current;
      const lastLine = lastTextLine(current);
      const page = await readShellJobOutput(sessionId, jobId, lastLine === null
        ? { maxLines: LOG_PAGE_LINES }
        : { fromLine: lastLine, maxLines: LOG_PAGE_LINES });
      if (cancelled || !page) return;
      const kept = lastLine === null ? [] : current.filter((row) => row.line < lastLine);
      let merged = [...kept, ...page.rows];
      if (merged.length > LOG_RETAINED_ROWS) {
        merged = merged.slice(merged.length - LOG_RETAINED_ROWS);
        setHasEarlier(true);
      }
      rowsRef.current = merged;
      setRows(merged);
      if (page.running || page.nextLine !== undefined) {
        timer = window.setTimeout(() => void follow().catch(report), page.nextLine !== undefined ? 0 : LOG_FOLLOW_MS);
      }
    };
    const report = (loadError: unknown) => {
      if (!cancelled) setError(loadError instanceof Error ? loadError.message : String(loadError));
    };
    void readShellJobOutput(sessionId, jobId, { maxLines: LOG_PAGE_LINES })
      .then((page) => {
        if (cancelled || !page) return;
        rowsRef.current = page.rows;
        setRows(page.rows);
        setHasEarlier(page.hasEarlier);
        setLoaded(true);
        if (page.running) timer = window.setTimeout(() => void follow().catch(report), LOG_FOLLOW_MS);
      })
      .catch(report);
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [jobId, running, sessionId]);

  React.useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    if (prependAnchorRef.current !== null) {
      element.scrollTop += element.scrollHeight - prependAnchorRef.current;
      prependAnchorRef.current = null;
      return;
    }
    if (followRef.current) element.scrollTop = element.scrollHeight;
  }, [rows]);

  // loadEarlier reads the page that ends right before the first row shown.
  // loadEarlier 读取恰好在第一条显示行之前结束的一页。
  const loadEarlier = React.useCallback(async () => {
    const first = rowsRef.current[0];
    if (!first || loadingEarlierRef.current) return;
    loadingEarlierRef.current = true;
    try {
      // A first row that continues a line restarts from that line, so the line is shown whole.
      // 第一行是某行的续行时，从该行重新开始，使该行完整显示。
      const beforeLine = first.continuation ? first.line + 1 : first.line;
      const page = await readShellJobOutput(sessionId, jobId, { beforeLine, maxLines: LOG_PAGE_LINES });
      if (!page) return;
      prependAnchorRef.current = scrollRef.current?.scrollHeight ?? null;
      const previous = rowsRef.current;
      const kept = first.continuation ? previous.filter((row) => row.line !== first.line) : previous;
      const merged = [...page.rows, ...kept];
      // Rows dropped from the bottom are read again by the follower or when the log is reopened.
      // 从底部丢弃的行会由跟随读取或重新打开日志时再次读取。
      const bounded = merged.length <= LOG_RETAINED_ROWS ? merged : merged.slice(0, LOG_RETAINED_ROWS);
      followRef.current = followRef.current && bounded.length === merged.length;
      rowsRef.current = bounded;
      setRows(bounded);
      setHasEarlier(page.hasEarlier);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      loadingEarlierRef.current = false;
    }
  }, [jobId, sessionId]);

  const handleScroll = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    followRef.current = element.scrollHeight - element.scrollTop - element.clientHeight <= LOG_EDGE_PX;
    if (element.scrollTop <= LOG_EDGE_PX && hasEarlier) void loadEarlier();
  }, [hasEarlier, loadEarlier]);

  return (
    <div className="vk-act-log" onScroll={handleScroll} ref={scrollRef}>
      {hasEarlier ? <div className="vk-act-log-more">{t("sessionActivity.scrollForEarlier")}</div> : null}
      {error ? <div className="vk-act-error">{error}</div> : null}
      {loaded && rows.length === 0 && !error ? <PageEmptyState title={t("sessionActivity.noOutput")} /> : null}
      {rows.map((row, index) => (
        <div
          className="vk-act-log-row"
          data-continuation={row.continuation ? "" : undefined}
          data-kind={row.kind}
          key={`${row.line}:${index}`}
        >
          {row.text}
        </div>
      ))}
    </div>
  );
}

// lastTextLine returns the line number of the last output row, or null when no row is shown.
// lastTextLine 返回最后一条输出行的行号；没有显示行时返回 null。
function lastTextLine(rows: ShellJobOutputRow[]): number | null {
  for (let index = rows.length - 1; index >= 0; index -= 1) {
    if (rows[index].kind === "text") return rows[index].line;
  }
  return null;
}
