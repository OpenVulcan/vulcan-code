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

  it("retains complete child buckets without child detail and does not double-count returned summaries", () => {
    const events = kimiPartialUsageEvents();
    const terminal = events[8].event;
    if (terminal.kind !== "turn_stats") throw new Error("fixture terminal");
    const child = { api_call_count: 2, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 1, usage: { cache_read_tokens: 80, uncached_input_tokens: 20 } };
    terminal.buckets.subagent = child;
    terminal.buckets.total.api_call_count = 10;
    expect(projectTurnUsageCoverage([events[8]]).get(9)!.total.dimensions.cache_read_tokens).toEqual({ knownTokens: 80, reportedApiCallCount: 2 });
    const childEvent = { ...events[0], event: { kind: "subagent_return_stats" as const, child_session_id: "child", child_task_id: "task", child_turn_stats_seq: 10, status: "final" as const, totals: child } };
    const coverage = projectTurnUsageCoverage([...events, childEvent, { ...childEvent, seq: 2 }]).get(9)!.total;
    expect(coverage.dimensions.cache_read_tokens).toEqual({ knownTokens: 179536, reportedApiCallCount: 9 });
    expect(coverage.expectedApiCallCount).toBe(10);
  });

  // Exercise child deduplication with unknown persisted totals so exact-bucket precedence cannot mask an error.
  // 使用未知持久化总量验证子代理去重，避免精确统计桶的优先级掩盖错误。
  it("matches first-return ownership and latest same-record revisions for partially known child usage", () => {
    // Isolate the direct API samples from the synthetic terminal record.
    // 将直接 API 样本与合成终态记录隔离。
    const events = kimiPartialUsageEvents();
    // The terminal bucket retains unknown child usage and the complete physical request count.
    // 终态统计桶保留未知子代理用量及完整物理请求数。
    const terminal = events.pop()!;
    if (terminal.event.kind !== "turn_stats") throw new Error("fixture terminal");
    terminal.seq = 12;
    terminal.event.end_seq = 12;
    terminal.event.buckets.subagent = { api_call_count: 3, unknown_usage_api_call_count: 1, tool_call_count: 0, subagent_return_count: 2, usage: {} };
    terminal.event.buckets.total = { api_call_count: 11, unknown_usage_api_call_count: 1, tool_call_count: 7, subagent_return_count: 2, usage: {} };
    // The first final return owns the contribution, weighted by its two physical requests.
    // 第一条最终回流拥有该贡献，按两次物理请求加权。
    const first = { ...events[0], seq: 9, event: {
      kind: "subagent_return_stats" as const, child_session_id: "child", child_task_id: "task", child_turn_stats_seq: 10, status: "final" as const,
      totals: { api_call_count: 2, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: { cache_read_tokens: 70, uncached_input_tokens: 20 } },
    } };
    // A revision of the same durable record supersedes that record's older body.
    // 同一可靠记录的修订替代该记录的旧正文。
    const revised = { ...first, event_revision: 2, event: { ...first.event, totals: { ...first.event.totals, usage: { cache_read_tokens: 80, uncached_input_tokens: 20 } } } };
    // A later duplicate return is ignored even if its values differ.
    // 较晚的重复回流即使数值不同，也必须忽略。
    const duplicate = { ...first, seq: 10, event: { ...first.event, totals: { ...first.event.totals, usage: { cache_read_tokens: 400, uncached_input_tokens: 100 } } } };
    // Another child's absent usage must not erase the first child's known contribution.
    // 另一子代理缺失用量不能抹去首个子代理的已知贡献。
    const unknown = { ...first, seq: 11, event: { ...first.event, child_session_id: "unknown-child", totals: { ...first.event.totals, api_call_count: 1, unknown_usage_api_call_count: 1, usage: {} } } };
    // Deliberately unordered replay validates identity and revision rules rather than array position.
    // 故意乱序的重放验证身份和修订规则，而非依赖数组位置。
    const coverage = projectTurnUsageCoverage([...events, duplicate, unknown, revised, first, terminal]).get(12)!;
    expect(coverage.subagent.dimensions.cache_read_tokens).toEqual({ knownTokens: 80, reportedApiCallCount: 2 });
    expect(coverage.subagent.cacheRatio).toEqual({ cachedTokens: 80, uncachedTokens: 20, reportedApiCallCount: 2 });
    expect(coverage.total.dimensions.cache_read_tokens).toEqual({ knownTokens: 179536, reportedApiCallCount: 9 });
    expect(coverage.total.expectedApiCallCount).toBe(11);
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
