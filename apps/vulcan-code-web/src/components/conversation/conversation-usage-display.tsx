import { formatTokenCount } from "@/lib/usage-format";
import type { UsageCoverage, UsageDimension } from "@/service/conversation-usage-coverage";

/** Derives output throughput from paired physical-request measurements.
 * 根据配对的物理请求测量计算输出吞吐率。
 * Parameters outputTokens and durationMs cover the same measured requests after their first token; returns tokens per second or null.
 * 参数 outputTokens 与 durationMs 覆盖同一组已测请求的首 Token 后输出；返回每秒 Token 数或 null。
 * Never pass total turn duration or tokens from unmeasured requests.
 * 不得传入整轮时长或未测请求的 Token。 */
export function averageTurnOutputSpeed(outputTokens: number | null | undefined, durationMs: number | null | undefined): number | null {
  if (outputTokens == null || durationMs == null || !Number.isFinite(outputTokens) || !Number.isFinite(durationMs) || outputTokens < 0 || durationMs <= 0) return null;
  return outputTokens / durationMs * 1000;
}

/** Formats `exact` when reported, otherwise the known contribution for `field`, without extra coverage annotations.
 * 已报告时格式化 `exact`，否则显示 `field` 的已知贡献，不附加覆盖说明。
 * `coverage` supplies view-only evidence; returns compact token text (exact below 1000, then K/M/B).
 * `coverage` 提供仅供视图使用的证据；返回紧凑 Token 文本（1000 以下精确，之后为 K/M/B）。
 * Missing values use a dash, while a reported zero remains zero; persisted accounting never changes.
 * 缺失值使用短横线，已上报零值仍为零；不改变持久化核算。 */
export function usageMetricDisplay(
  exact: number | null | undefined,
  field: UsageDimension,
  coverage: UsageCoverage | null | undefined,
): string {
  // Only a positive reported count proves the presence of a value, including explicit zero.
  // 只有正数上报次数才能证明数值存在，也包括明确零值。
  const known = coverage?.dimensions[field];
  // Exact totals take precedence without overwriting or persisting partial evidence.
  // 精确总量优先，不覆盖或持久化部分证据。
  const value = exact ?? (known && known.reportedApiCallCount > 0 ? known.knownTokens : null);
  return value === null ? "-" : formatTokenCount(value);
}

/** Computes a cache ratio only from exact totals or paired values of the same known requests.
 * 仅从精确总量或同一批已知请求的配对值计算缓存命中率。
 * `cached` and `uncached` are optional exact totals; `coverage` supplies paired evidence when exact totals are incomplete.
 * `cached` 和 `uncached` 是可选精确总量；精确总量不全时，`coverage` 提供配对证据。
 * Returns the ratio and whether its cohort omits any expected requests; never mixes independent partial sums.
 * 返回比率及样本组是否缺少预期请求；绝不混合独立的部分累计值。 */
export function usageCacheRatio(
  cached: number | null | undefined,
  uncached: number | null | undefined,
  coverage: UsageCoverage | null | undefined,
): { percent: number | null; partial: boolean } {
  // Both exact components must exist before selecting the complete-turn denominator.
  // 只有两个精确分项都存在，才选择完整回合分母。
  const exact = cached != null && uncached != null;
  // The cohort explicitly excludes requests missing either input component.
  // 样本组明确排除缺少任一输入分项的请求。
  const cohort = coverage?.cacheRatio;
  // Never combine an exact cached total with a partial uncached total.
  // 绝不将精确缓存总量与部分无缓存累计混用。
  const cachedCount = exact ? cached : cohort && cohort.reportedApiCallCount > 0 ? cohort.cachedTokens : null;
  // Use the identical cohort selected for cached input.
  // 使用与缓存输入完全相同的样本组。
  const uncachedCount = exact ? uncached : cohort && cohort.reportedApiCallCount > 0 ? cohort.uncachedTokens : null;
  // An empty or absent input denominator does not imply a measured zero-percent hit rate.
  // 空或缺失的输入分母不代表测得零命中率。
  const denominator = cachedCount == null || uncachedCount == null ? 0 : cachedCount + uncachedCount;
  return {
    percent: denominator > 0 && cachedCount != null ? Math.min(100, Math.max(0, cachedCount / denominator * 100)) : null,
    partial: !exact && Boolean(coverage && cohort && cohort.reportedApiCallCount > 0 && cohort.reportedApiCallCount < coverage.expectedApiCallCount),
  };
}
