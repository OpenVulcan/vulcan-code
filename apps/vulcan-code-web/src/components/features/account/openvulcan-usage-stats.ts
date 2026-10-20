import type { OpenVulcanUsagePage } from "@/service/openvulcan-account-service";

// Usage statistics fold the server's hourly pages into totals, a per-day trend and a per-model breakdown.
// Every counter stays an exact BigInt; only bar proportions become small Numbers.
// 用量统计将服务端小时分页折叠为合计、每日趋势与按模型分布。
// 所有计数保持精确 BigInt；只有柱条比例转为很小的 Number。

// STATS_PAGE_LIMIT bounds how many usage pages one statistics read follows, so a busy account never
// turns a refresh into an unbounded download; a capped read is reported as partial.
// STATS_PAGE_LIMIT 限制一次统计读取跟随的用量页数，避免繁忙账户的刷新变成无界下载；达到上限时标记为部分统计。
export const STATS_PAGE_LIMIT = 20;

// UsageCounters is one exact set of counters for a day, a model or the whole range.
// UsageCounters 是某一天、某个模型或整个区间的一组精确计数。
export interface UsageCounters {
  requests: bigint;
  failed: bigint;
  tokens: bigint;
  microPoints: bigint;
}

// UsageStats is the folded statistics of one server date range.
// UsageStats 是一个服务端日期区间的折叠统计。
export interface UsageStats {
  // Inclusive server-owned date range of the first page.
  // 第一页的服务端包含式日期区间。
  fromDate: string;
  toDate: string;
  totals: UsageCounters;
  // Every date in the range, oldest first, including days without usage.
  // 区间内每一天（含无用量的日期），由旧到新。
  days: Array<{ date: string; counters: UsageCounters }>;
  // Models ordered by charge, then requests, then id.
  // 模型按扣费、请求次数、标识依次排序。
  models: Array<{ modelId: string; counters: UsageCounters }>;
  // True when more pages existed beyond STATS_PAGE_LIMIT.
  // 超出 STATS_PAGE_LIMIT 后仍有更多页时为 true。
  partial: boolean;
}

// emptyCounters returns a fresh zero counter set.
// emptyCounters 返回新的零值计数。
function emptyCounters(): UsageCounters {
  return { requests: 0n, failed: 0n, tokens: 0n, microPoints: 0n };
}

// exact parses a non-negative integer string, treating anything else as zero.
// exact 解析非负整数字符串，其他值视为零。
function exact(value: string): bigint {
  return /^\d+$/.test(value) ? BigInt(value) : 0n;
}

// datesBetween lists every ISO date from from to to inclusive, or an empty list for invalid input.
// datesBetween 列出 from 至 to（含）之间的每个 ISO 日期；输入无效时返回空列表。
function datesBetween(from: string, to: string): string[] {
  const start = Date.parse(`${from}T00:00:00Z`);
  const end = Date.parse(`${to}T00:00:00Z`);
  if (Number.isNaN(start) || Number.isNaN(end) || end < start) return [];
  const dates: string[] = [];
  for (let time = start; time <= end; time += 86_400_000) dates.push(new Date(time).toISOString().slice(0, 10));
  return dates;
}

// foldUsagePages folds already-read pages into statistics; the first page owns the date range.
// foldUsagePages 将已读取的分页折叠为统计；第一页决定日期区间。
// Parameters: pages are in read order; partial says whether reading stopped at the page limit.
// 参数：pages 按读取顺序排列；partial 表示读取是否因页数上限而停止。
export function foldUsagePages(pages: readonly OpenVulcanUsagePage[], partial: boolean): UsageStats {
  const fromDate = pages[0]?.from_date ?? "";
  const toDate = pages[0]?.to_date ?? "";
  const totals = emptyCounters();
  const days = new Map(datesBetween(fromDate, toDate).map((date) => [date, emptyCounters()]));
  const models = new Map<string, UsageCounters>();
  for (const row of pages.flatMap((page) => page.usage)) {
    const delta = {
      requests: exact(row.request_count),
      failed: exact(row.failed_requests),
      tokens: exact(row.input_tokens),
      microPoints: exact(row.charged_micro_points),
    };
    const date = row.hour_start.slice(0, 10);
    if (!days.has(date)) days.set(date, emptyCounters());
    if (!models.has(row.model_id)) models.set(row.model_id, emptyCounters());
    for (const target of [totals, days.get(date)!, models.get(row.model_id)!]) {
      target.requests += delta.requests;
      target.failed += delta.failed;
      target.tokens += delta.tokens;
      target.microPoints += delta.microPoints;
    }
  }
  const order = (left: bigint, right: bigint) => (left === right ? 0 : left > right ? -1 : 1);
  return {
    fromDate,
    toDate,
    totals,
    days: [...days].sort(([left], [right]) => left.localeCompare(right)).map(([date, counters]) => ({ date, counters })),
    models: [...models]
      .map(([modelId, counters]) => ({ modelId, counters }))
      .sort((left, right) => order(left.counters.microPoints, right.counters.microPoints)
        || order(left.counters.requests, right.counters.requests)
        || left.modelId.localeCompare(right.modelId)),
    partial,
  };
}

// readUsageStats follows the usage cursor from the newest page up to STATS_PAGE_LIMIT pages.
// readUsageStats 从最新一页开始沿用量游标读取，最多 STATS_PAGE_LIMIT 页。
// Parameters: loadPage reads one page by opaque cursor. Returns the folded statistics.
// 参数：loadPage 按不透明游标读取一页。返回折叠后的统计。
export async function readUsageStats(loadPage: (cursor: string) => Promise<OpenVulcanUsagePage>): Promise<UsageStats> {
  const pages: OpenVulcanUsagePage[] = [];
  let cursor = "";
  do {
    const page = await loadPage(cursor);
    pages.push(page);
    cursor = page.next_cursor;
  } while (cursor && pages.length < STATS_PAGE_LIMIT);
  return foldUsagePages(pages, cursor !== "");
}

// shareOf returns part/whole as a 0–100 percentage for a bar, or 0 when whole is zero.
// shareOf 以 0–100 百分比返回 part/whole，用于柱条；whole 为零时返回 0。
export function shareOf(part: bigint, whole: bigint): number {
  if (whole <= 0n) return 0;
  // Per-mille precision keeps the division in BigInt; only the small quotient becomes a Number.
  // 千分位精度使除法保留在 BigInt 中；只有很小的商转为 Number。
  return Number((part > whole ? whole : part) * 1000n / whole) / 10;
}
