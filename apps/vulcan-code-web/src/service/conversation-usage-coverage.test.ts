import { describe, expect, it } from "vitest";
import { kimiPartialUsageEvents } from "./__fixtures__/kimi-partial-usage";
import { projectTurnUsageCoverage, readUsageCoverage } from "./conversation-usage-coverage";
import { usageCacheRatio } from "@/components/conversation/conversation-usage-display";

describe("view-only usage coverage", () => {
  it("preserves seven known Kimi cache samples without changing old persisted totals", () => {
    const events = kimiPartialUsageEvents();
    const original = JSON.stringify(events);
    const coverage = projectTurnUsageCoverage(events).get(9)!;
    expect(coverage.total.expectedApiCallCount).toBe(8);
    expect(coverage.total.dimensions.cache_read_tokens).toEqual({ knownTokens: 179456, reportedApiCallCount: 7 });
    expect(coverage.total.dimensions.uncached_input_tokens).toEqual({ knownTokens: 16090, reportedApiCallCount: 7 });
    expect(usageCacheRatio(undefined, undefined, coverage.total)).toEqual({ percent: 179456 / 195546 * 100, partial: true });
    expect(coverage.total.dimensions.total_tokens).toEqual({ knownTokens: 214777, reportedApiCallCount: 8 });
    expect(JSON.stringify(events)).toBe(original);
    expect(readUsageCoverage(JSON.parse(JSON.stringify(coverage.total)))).toEqual(coverage.total);
  });

  it("never promotes a partial loaded window into full coverage", () => {
    const events = kimiPartialUsageEvents();
    const coverage = projectTurnUsageCoverage(events.slice(-2)).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens).toEqual({ knownTokens: 31744, reportedApiCallCount: 1 });
    expect(coverage.expectedApiCallCount).toBe(8);
    expect(coverage.dimensions.output_tokens).toEqual({ knownTokens: 1089, reportedApiCallCount: 8 });
    expect(projectTurnUsageCoverage(events.slice(-1)).get(9)!.total.dimensions.cache_read_tokens.reportedApiCallCount).toBe(0);
  });

  it("counts explicit zero as reported but never treats absence as zero", () => {
    const events = kimiPartialUsageEvents();
    for (const event of events) if (event.event.kind === "api_stats") event.event.usage = {};
    if (events[0].event.kind === "api_stats") events[0].event.usage = { cache_read_tokens: 0, uncached_input_tokens: 30 };
    const coverage = projectTurnUsageCoverage(events).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens).toEqual({ knownTokens: 0, reportedApiCallCount: 1 });
    expect(usageCacheRatio(undefined, undefined, coverage)).toEqual({ percent: 0, partial: true });
  });

  it("does not pair cache and uncached fields from different API requests", () => {
    const events = kimiPartialUsageEvents();
    for (const event of events) if (event.event.kind === "api_stats") event.event.usage = {};
    if (events[0].event.kind === "api_stats") events[0].event.usage = { cache_read_tokens: 10 };
    if (events[1].event.kind === "api_stats") events[1].event.usage = { uncached_input_tokens: 5 };
    const coverage = projectTurnUsageCoverage(events).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens.knownTokens).toBe(10);
    expect(coverage.cacheRatio.reportedApiCallCount).toBe(0);
    expect(usageCacheRatio(undefined, undefined, coverage).percent).toBeNull();
  });

  it("removes the partial marker only when every request has an explicit cache sample", () => {
    const events = kimiPartialUsageEvents();
    if (events[0].event.kind === "api_stats") {
      events[0].event.usage.cache_read_tokens = 0;
      events[0].event.usage.uncached_input_tokens = 18142;
    }
    const coverage = projectTurnUsageCoverage(events).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens.reportedApiCallCount).toBe(8);
    expect(usageCacheRatio(undefined, undefined, coverage)).toEqual({ percent: 179456 / 213688 * 100, partial: false });
  });

  it("preserves no-data semantics when no request reports cache fields", () => {
    const events = kimiPartialUsageEvents();
    for (const summary of events) if (summary.event.kind === "api_stats") {
      delete summary.event.usage.cache_read_tokens;
      delete summary.event.usage.uncached_input_tokens;
    }
    const coverage = projectTurnUsageCoverage(events).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens).toEqual({ knownTokens: 0, reportedApiCallCount: 0 });
    expect(usageCacheRatio(undefined, undefined, coverage).percent).toBeNull();
  });

  it("deduplicates reconnect API revisions and isolates other turns", () => {
    const events = kimiPartialUsageEvents();
    const repeated = { ...events[1], event_revision: 2 };
    expect(projectTurnUsageCoverage([...events, repeated, { ...events[2], turn_id: 99 }]).get(9)!.total.dimensions.cache_read_tokens)
      .toEqual({ knownTokens: 179456, reportedApiCallCount: 7 });
  });

  it("retains complete child buckets without child detail", () => {
    const events = kimiPartialUsageEvents();
    const terminal = events[8].event;
    if (terminal.kind !== "turn_stats") throw new Error("fixture terminal");
    const child = { api_call_count: 2, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 1, usage: { cache_read_tokens: 80, uncached_input_tokens: 20 } };
    terminal.buckets.subagent = child;
    terminal.buckets.total.api_call_count = 10;
    expect(projectTurnUsageCoverage([events[8]]).get(9)!.total.dimensions.cache_read_tokens).toEqual({ knownTokens: 80, reportedApiCallCount: 2 });
  });

  // Aggregate timing remains weighted and does not disappear when child API details are unloaded.
  // 聚合计时保持加权，且不会因子代理 API 明细未加载而消失。
  it("uses own timing buckets even when historical inclusive timing differs", () => {
    const events = kimiPartialUsageEvents();
    const terminal = events[8].event;
    if (terminal.kind !== "turn_stats") throw new Error("fixture terminal");
    terminal.buckets.direct.output_timing = {
      total_request_count: 3, total_request_duration_ms: 7000,
      request_count: 3, request_duration_ms: 7000,
      first_token_latency_ms: 1000, output_duration_ms: 6000, output_tokens: 120,
    };
    terminal.buckets.total.output_timing = { ...terminal.buckets.direct.output_timing, request_count: 99, output_tokens: 99999 };
    const timing = projectTurnUsageCoverage([events[8]]).get(9)!.outputTiming!;
    expect(timing.requestCount).toBe(3);
    expect(timing.firstTokenLatencyMs).toBe(1000);
    expect(timing.outputTokens * 1000 / timing.outputDurationMs).toBe(20);
    expect(projectTurnUsageCoverage(events).get(9)!.outputTiming).toEqual(timing);
  });

  it("includes compaction and subagent-purpose work owned by the same session", () => {
    const events = kimiPartialUsageEvents();
    if (events[1].event.kind === "api_stats") events[1].event.purpose = "compaction";
    if (events[2].event.kind === "api_stats") events[2].event.purpose = "subagent";
    expect(projectTurnUsageCoverage(events).get(9)!.total.dimensions.cache_read_tokens.knownTokens).toBe(179456);
  });

  it("rejects malformed coverage metadata without guessing field aliases", () => {
    expect(readUsageCoverage(null)).toBeNull();
    expect(readUsageCoverage({ expectedApiCallCount: 8, dimensions: {} })).toBeNull();
    const coverage = projectTurnUsageCoverage(kimiPartialUsageEvents()).get(9)!.total;
    coverage.dimensions.cache_read_tokens.reportedApiCallCount = 9;
    expect(readUsageCoverage(coverage)).toBeNull();
  });
});
