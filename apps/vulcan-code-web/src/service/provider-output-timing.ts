import type { ApiTimings } from "./conversation-event-window-store";

/** Measured output cohort; tokens and duration always belong to the same physical requests.
 * 已测输出样本组；Token 与时长始终来自相同的物理请求。 */
export interface ProviderOutputTiming {
  /** Number of requests with all three ordered timing checkpoints and output usage.
   * 同时具备三个有序时间点及输出用量的请求数。 */
  requestCount: number;
  /** Sum of request dispatch-to-completion durations in milliseconds.
   * 请求分派到完成的累计毫秒时长。 */
  requestDurationMs: number;
  /** Sum of dispatch-to-first-token latencies in milliseconds.
   * 分派到首 Token 的累计毫秒延迟。 */
  firstTokenLatencyMs: number;
  /** Sum of first-token-to-completion durations in milliseconds, excluding tool execution between requests.
   * 首 Token 到完成的累计毫秒时长，不含请求之间的工具执行。 */
  outputDurationMs: number;
  /** Output tokens reported by precisely this measured cohort.
   * 由该已测样本组精确报告的输出 Token。 */
  outputTokens: number;
}

/** Returns one physical request's timing cohort, or null when timing or output usage is unavailable.
 * 返回一次物理请求的计时样本；时间或输出用量缺失时返回 null。
 * timing supplies persisted checkpoints; outputTokens is the same request's reported output count.
 * timing 提供持久化时间点；outputTokens 是同次请求报告的输出数。
 * Empty or inverted output intervals cannot establish speed and are never replaced with total turn time.
 * 空或倒序输出区间不能证明速度，绝不以整轮时间替代。 */
export function measuredProviderOutput(timing: ApiTimings, outputTokens: number | undefined): ProviderOutputTiming | null {
  // Only exact dispatch, token and completion observations define the measurement interval.
  // 仅精确的分派、Token 和完成观测定义测量区间。
  const { dispatched_at_ms: start, first_token_at_ms: first, completed_at_ms: end } = timing;
  if (start === undefined || first === undefined || end === undefined || outputTokens === undefined
    || !Number.isFinite(start) || !Number.isFinite(first) || !Number.isFinite(end)
    || !Number.isFinite(outputTokens) || outputTokens < 0 || first < start || end <= first) return null;
  return { requestCount: 1, requestDurationMs: end - start, firstTokenLatencyMs: first - start,
    outputDurationMs: end - first, outputTokens };
}
