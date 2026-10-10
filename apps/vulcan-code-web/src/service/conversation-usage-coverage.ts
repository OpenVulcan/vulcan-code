import type { AggregatedUsage, ConversationEventSummary, StatBucket } from "./conversation-event-window-store";
import { measuredProviderOutput, type ProviderOutputTiming } from "./provider-output-timing";

/** Token dimensions whose partial sums are display evidence, never budget totals.
 * 部分累计仅作为展示证据、绝不作为预算总量的 Token 维度。 */
export const USAGE_DIMENSIONS = [
  "input_tokens", "uncached_input_tokens", "output_tokens", "reasoning_tokens", "total_tokens",
  "cache_read_tokens", "cache_write_tokens", "audio_input_tokens", "audio_output_tokens",
] as const satisfies readonly (keyof AggregatedUsage)[];

/** One canonical numeric usage field.
 * 一个规范数值用量字段。 */
export type UsageDimension = typeof USAGE_DIMENSIONS[number];

/** Known contribution and the number of physical requests proving it.
 * 已知贡献量及能证明它的物理请求数。 */
export interface UsageDimensionCoverage {
  /** Sum supported by the known samples, not necessarily a complete total.
   * 已知样本支持的累计值，不一定是完整总量。 */
  knownTokens: number;
  /** Number of physical requests whose value for this dimension is known.
   * 当前维度的数值已知的物理请求数。 */
  reportedApiCallCount: number;
}

/** A cache ratio cohort where both cached and uncached counts are known for the same requests.
 * 同一批请求的缓存与无缓存数均已知的命中率样本组。 */
export interface CacheRatioCoverage {
  /** Cached input contributed by requests with both input components known.
   * 两种输入分项均已知的请求贡献的缓存输入。 */
  cachedTokens: number;
  /** Uncached input from exactly the same request cohort.
   * 完全相同请求样本组贡献的无缓存输入。 */
  uncachedTokens: number;
  /** Number of requests in the paired cohort.
   * 配对样本组中的请求数。 */
  reportedApiCallCount: number;
}

/** Derived view-only coverage; absent persisted dimensions retain their original meaning.
 * 仅用于视图的派生覆盖信息；持久化缺失维度保持原语义。 */
export interface UsageCoverage {
  /** Complete request count from the persisted bucket, including unknown samples.
   * 来自持久化统计桶的完整请求数，包含未知样本。 */
  expectedApiCallCount: number;
  /** Independently tracked known evidence for each canonical dimension.
   * 各规范维度独立追踪的已知证据。 */
  dimensions: Record<UsageDimension, UsageDimensionCoverage>;
  /** Paired evidence kept separate from independent per-dimension sums.
   * 与各维度独立累计分开保存的配对证据。 */
  cacheRatio: CacheRatioCoverage;
}

/** Coverage scopes corresponding exactly to the persisted Turn buckets.
 * 与持久化 Turn 统计桶精确对应的覆盖范围。 */
export interface TurnUsageCoverage {
  /** Direct API timing cohort; child aggregates do not contain physical timing and are excluded.
   * 直接 API 计时样本；子代理聚合不含物理计时，因此不纳入。 */
  outputTiming?: ProviderOutputTiming;
  /** Requests owned by this session, including compaction work.
   * 当前会话拥有的请求，包含压缩工作。 */
  direct: UsageCoverage;
  /** Final child-return evidence, deduplicated by child session and terminal seq.
   * 按子会话和终态 seq 去重后的最终子代理回流证据。 */
  subagent: UsageCoverage;
  /** Inclusive evidence across the direct and child scopes.
   * 包含直接及子代理范围的总计证据。 */
  total: UsageCoverage;
}

/** Metadata envelope needed for accounting, shared by raw windows and display projections.
 * 原始窗口与显示投影共享的核算元数据封套。 */
type UsageEvent = Pick<ConversationEventSummary, "seq" | "turn_id" | "event_revision" | "event">;

/** Creates an empty accumulator for a persisted expected request count; never implies a reported zero.
 * 为持久化的预期请求数创建空累加器；不代表上游报告零值。 */
function emptyCoverage(expectedApiCallCount: number): UsageCoverage {
  return {
    expectedApiCallCount,
    dimensions: Object.fromEntries(USAGE_DIMENSIONS.map((field) => [field, { knownTokens: 0, reportedApiCallCount: 0 }])) as UsageCoverage["dimensions"],
    cacheRatio: { cachedTokens: 0, uncachedTokens: 0, reportedApiCallCount: 0 },
  };
}

/** Adds one normalized API or fully-known child aggregate with its physical request weight.
 * 按物理请求权重追加一笔规范 API 或完整已知的子代理聚合。
 * `coverage` is the mutable view accumulator; `usage` contains canonical optional counts; `requestCount` is its request weight.
 * `coverage` 是可变视图累加器；`usage` 包含规范可选计数；`requestCount` 是请求权重。
 * Missing fields contribute no evidence; paired cache evidence is never assembled across different requests.
 * 缺失字段不贡献证据；缓存配对证据绝不跨不同请求拼接。
 * Updates only the accumulator and returns no value.
 * 仅更新累加器，无返回值。 */
function addUsage(coverage: UsageCoverage, usage: AggregatedUsage, requestCount: number): void {
  for (const field of USAGE_DIMENSIONS) {
    // Preserve the distinction between an explicit zero and an absent dimension.
    // 保留明确零值与缺失维度的区别。
    const tokens = usage[field];
    if (tokens === undefined) continue;
    coverage.dimensions[field].knownTokens += tokens;
    coverage.dimensions[field].reportedApiCallCount += requestCount;
  }
  if (usage.cache_read_tokens !== undefined && usage.uncached_input_tokens !== undefined) {
    coverage.cacheRatio.cachedTokens += usage.cache_read_tokens;
    coverage.cacheRatio.uncachedTokens += usage.uncached_input_tokens;
    coverage.cacheRatio.reportedApiCallCount += requestCount;
  }
}

/** Uses exact persisted totals when available, otherwise retains only proven partial evidence.
 * 存在精确持久化总量时使用它，否则仅保留已证明的部分证据。
 * `coverage` is updated from the authoritative `bucket` and returned without modifying the bucket.
 * 根据权威 `bucket` 更新并返回 `coverage`，不修改统计桶。
 * This also supports historical windows containing only terminal statistics, without inventing missing API samples.
 * 同样支持仅含终态统计的历史窗口，不编造缺失 API 样本。 */
function finalizeCoverage(coverage: UsageCoverage, bucket: StatBucket): UsageCoverage {
  for (const field of USAGE_DIMENSIONS) {
    if (bucket.usage[field] !== undefined) {
      coverage.dimensions[field] = { knownTokens: bucket.usage[field], reportedApiCallCount: bucket.api_call_count };
    }
  }
  if (bucket.usage.cache_read_tokens !== undefined && bucket.usage.uncached_input_tokens !== undefined) {
    coverage.cacheRatio = {
      cachedTokens: bucket.usage.cache_read_tokens,
      uncachedTokens: bucket.usage.uncached_input_tokens,
      reportedApiCallCount: bucket.api_call_count,
    };
  }
  return coverage;
}

/** Projects partial usage from canonical events, keyed by terminal Turn-stat seq.
 * 从规范事件投影部分用量，以终态 Turn 统计 seq 为键。
 * `events` may contain a partial window; persisted bucket counts remain the denominator, and source records are never mutated.
 * `events` 可以是局部窗口；分母始终使用持久化统计桶请求数，绝不修改源记录。
 * Returns direct/subagent/total evidence with API-ID deduplication; child evidence comes from persisted buckets.
 * 返回直接/子代理/总计证据，按 API ID 去重；子代理证据来自持久化统计桶。 */
export function projectTurnUsageCoverage(events: readonly UsageEvent[]): ReadonlyMap<number, TurnUsageCoverage> {
  // byTurn limits each closed Turn to its own canonical API facts.
  // byTurn 将闭合 Turn 限制在自己的规范 API 事实内。
  const byTurn = new Map<number, UsageEvent[]>();
  for (const event of events) {
    if (event.turn_id === null) continue;
    // Each list belongs to one stable Turn identity rather than visual grouping.
    // 每个列表归属于一个稳定 Turn 身份，而非视觉分组。
    const turnEvents = byTurn.get(event.turn_id) ?? [];
    turnEvents.push(event);
    byTurn.set(event.turn_id, turnEvents);
  }
  // Terminal seq is the stable lookup key used by both display adapters.
  // 终态 seq 是两条展示适配路径共同使用的稳定查询键。
  const result = new Map<number, TurnUsageCoverage>();
  for (const terminal of events) {
    if (terminal.event.kind !== "turn_stats" || terminal.turn_id === null) continue;
    // Persisted boundaries define ownership and expected counts for this terminal record.
    // 持久化边界定义该终态记录的归属范围和预期计数。
    const { buckets, start_seq, end_seq } = terminal.event;
    // Direct evidence includes every API purpose owned by this Turn.
    // 直接证据包含当前 Turn 拥有的全部 API 用途。
    const direct = emptyCoverage(buckets.direct.api_call_count);
    // Child evidence is weighted by physical request counts in each final return.
    // 子代理证据按各最终回流中的物理请求数加权。
    const subagent = emptyCoverage(buckets.subagent.api_call_count);
    // Inclusive evidence is assembled only after exact per-scope totals are applied.
    // 总计证据仅在应用各范围的精确总量后组装。
    const total = emptyCoverage(buckets.total.api_call_count);
    // API identity prevents reconnect copies from being counted twice.
    // API 身份防止重连副本被重复计数。
    const apis = new Map<number, UsageEvent>();
    // Accumulate tokens and output intervals from the same measured physical request set.
    // 从同一组已测物理请求累计 Token 和输出区间。
    const outputTiming: ProviderOutputTiming = { requestCount: 0, requestDurationMs: 0, firstTokenLatencyMs: 0, outputDurationMs: 0, outputTokens: 0 };
    for (const sample of byTurn.get(terminal.turn_id) ?? []) {
      if (sample.seq < start_seq || sample.seq > end_seq) continue;
      if (sample.event.kind === "api_stats") {
        // Keep the newest revision of an API terminal fact when a window contains replayed copies.
        // 窗口包含重放副本时，保留 API 终态事实的最新修订。
        const previous = apis.get(sample.event.api_call_id);
        if (!previous || sample.seq > previous.seq || (sample.seq === previous.seq && sample.event_revision >= previous.event_revision)) {
          apis.set(sample.event.api_call_id, sample);
        }
      }
    }
    for (const sample of apis.values()) {
      if (sample.event.kind !== "api_stats") continue;
      addUsage(direct, sample.event.usage, 1);
      // Never include output tokens from requests whose generation interval is unknown.
      // 不纳入生成区间未知的请求所产生的输出 Token。
      const timing = measuredProviderOutput(sample.event.timing, sample.event.usage.output_tokens);
      if (timing !== null) {
        outputTiming.requestCount += timing.requestCount;
        outputTiming.requestDurationMs += timing.requestDurationMs;
        outputTiming.firstTokenLatencyMs += timing.firstTokenLatencyMs;
        outputTiming.outputDurationMs += timing.outputDurationMs;
        outputTiming.outputTokens += timing.outputTokens;
      }
    }
    // The canonical own bucket covers local timing outside the currently loaded window.
    // 规范自身统计桶覆盖当前已加载窗口之外的本地计时。
    const timing = buckets.direct.output_timing;
    if (timing) {
      outputTiming.requestCount = timing.request_count;
      outputTiming.requestDurationMs = timing.request_duration_ms;
      outputTiming.firstTokenLatencyMs = timing.first_token_latency_ms;
      outputTiming.outputDurationMs = timing.output_duration_ms;
      outputTiming.outputTokens = timing.output_tokens;
    }
    finalizeCoverage(direct, buckets.direct);
    finalizeCoverage(subagent, buckets.subagent);
    // Complete persisted child/direct buckets remain usable even when their detailed events are outside the loaded window.
    // 即使明细事件不在已加载窗口内，完整的持久化子代理/直接统计桶仍然可用。
    for (const field of USAGE_DIMENSIONS) {
      total.dimensions[field] = {
        knownTokens: direct.dimensions[field].knownTokens + subagent.dimensions[field].knownTokens,
        reportedApiCallCount: direct.dimensions[field].reportedApiCallCount + subagent.dimensions[field].reportedApiCallCount,
      };
    }
    total.cacheRatio = {
      cachedTokens: direct.cacheRatio.cachedTokens + subagent.cacheRatio.cachedTokens,
      uncachedTokens: direct.cacheRatio.uncachedTokens + subagent.cacheRatio.uncachedTokens,
      reportedApiCallCount: direct.cacheRatio.reportedApiCallCount + subagent.cacheRatio.reportedApiCallCount,
    };
    result.set(terminal.seq, {
      outputTiming,
      direct,
      subagent,
      total: finalizeCoverage(total, buckets.total),
    });
  }
  return result;
}

/** Validates view metadata crossing the legacy unknown-metadata boundary; returns null for an absent/invalid value.
 * 校验跨旧版 unknown 元数据边界的视图元数据；缺失或无效时返回 null。 */
export function readUsageCoverage(value: unknown): UsageCoverage | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  // The legacy metadata boundary is untyped, so each numeric field is checked before use.
  // 旧元数据边界没有类型保证，因此使用前校验每个数值字段。
  const coverage = value as Partial<UsageCoverage>;
  // This count is the denominator for every dimension and the paired cohort.
  // 该计数是所有维度及配对样本组的分母。
  const count = coverage.expectedApiCallCount;
  if (!isCount(count) || !coverage.dimensions || !coverage.cacheRatio) return null;
  for (const field of USAGE_DIMENSIONS) {
    // Reject malformed evidence instead of guessing aliases or clamping request counts.
    // 拒绝无效证据，不猜测别名或截断请求计数。
    const dimension = coverage.dimensions[field];
    if (!dimension || !isCount(dimension.knownTokens) || !isCount(dimension.reportedApiCallCount) || dimension.reportedApiCallCount > count) return null;
  }
  // Paired evidence has its own coverage and cannot exceed the complete request count.
  // 配对证据有独立覆盖数，不能超过完整请求数。
  const ratio = coverage.cacheRatio;
  if (!isCount(ratio.cachedTokens) || !isCount(ratio.uncachedTokens) || !isCount(ratio.reportedApiCallCount) || ratio.reportedApiCallCount > count) return null;
  return coverage as UsageCoverage;
}

/** Checks one non-negative safe integer used by local accounting evidence.
 * 校验本地核算证据使用的非负安全整数。 */
function isCount(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}
