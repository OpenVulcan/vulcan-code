import { describe, expect, it, vi } from "vitest";
import type { OpenVulcanUsagePage } from "@/service/openvulcan-account-service";
import { foldUsagePages, readUsageStats, shareOf, STATS_PAGE_LIMIT } from "./openvulcan-usage-stats";

// row builds one hourly aggregate with exact decimal strings.
// row 构造一条使用精确十进制字符串的小时聚合。
function row(hour: string, model: string, requests: string, failed: string, tokens: string, micro: string): OpenVulcanUsagePage["usage"][number] {
  return { hour_start: hour, model_id: model, request_count: requests, successful_requests: "0", failed_requests: failed, input_tokens: tokens, charged_micro_points: micro };
}

// page builds one usage page over a fixed range.
// page 构造一个固定区间的用量页。
function page(usage: OpenVulcanUsagePage["usage"], next = ""): OpenVulcanUsagePage {
  return { from_date: "2026-10-18", to_date: "2026-10-20", next_cursor: next, usage };
}

describe("foldUsagePages", () => {
  it("sums exact totals, fills every day of the range and orders models by charge then requests", () => {
    const stats = foldUsagePages([
      page([row("2026-10-20T11:00:00Z", "embed", "5", "0", "262", "26200")]),
      page([row("2026-10-18T01:00:00Z", "embed", "1", "1", "23", "2300"), row("2026-10-18T02:00:00Z", "router", "3", "2", "9007199254740993", "927584")]),
    ], false);
    expect(stats.totals).toEqual({ requests: 9n, failed: 3n, tokens: 9007199254741278n, microPoints: 956084n });
    expect(stats.days.map((day) => [day.date, day.counters.requests])).toEqual([["2026-10-18", 4n], ["2026-10-19", 0n], ["2026-10-20", 5n]]);
    expect(stats.models.map((model) => model.modelId)).toEqual(["router", "embed"]);
    expect(stats.partial).toBe(false);
  });
});

describe("readUsageStats", () => {
  it("follows the cursor and stops at the page limit, reporting a partial read", async () => {
    const loadPage = vi.fn(async (cursor: string) => page([row("2026-10-19T00:00:00Z", "m", "1", "0", "1", "1")], `${cursor}+`));
    const stats = await readUsageStats(loadPage);
    expect(loadPage).toHaveBeenCalledTimes(STATS_PAGE_LIMIT);
    expect(loadPage).toHaveBeenNthCalledWith(2, "+");
    expect(stats.partial).toBe(true);
    expect(stats.totals.requests).toBe(BigInt(STATS_PAGE_LIMIT));
  });
});

describe("shareOf", () => {
  it("returns a bounded percentage and zero for an empty whole", () => {
    expect(shareOf(1n, 3n)).toBe(33.3);
    expect(shareOf(5n, 3n)).toBe(100);
    expect(shareOf(1n, 0n)).toBe(0);
  });
});
