/** A measured sum and its reporting sample count.
 * 一个测量总和及其上报样本数。 */
export interface UsageMeasurement {
  /** Sum of known values.
   * 已知值之和。 */
  sum: number;
  /** Requests reporting this dimension.
   * 上报当前维度的请求数。 */
  samples: number;
}

/** Additive own-session counters from the Core usage owner.
 * 来自 Core 用量所有者的会话自身累计。 */
export interface SessionUsageBucket {
  /** Completed physical request count.
   * 已结束的物理请求数。 */
  requestCount: number;
  /** Requests without token usage.
   * 没有 Token 用量的请求数。 */
  unknownUsageCount: number;
  /** Prepared tool call count.
   * 已准备的工具调用数。 */
  toolCallCount: number;
  /** Provider input totals.
   * Provider 输入累计。 */
  inputTokens: UsageMeasurement;
  /** Uncached input totals.
   * 未缓存输入累计。 */
  uncachedInputTokens: UsageMeasurement;
  /** Cache read totals.
   * 缓存读取累计。 */
  cacheReadTokens: UsageMeasurement;
  /** Cache write totals.
   * 缓存写入累计。 */
  cacheWriteTokens: UsageMeasurement;
  /** Output totals.
   * 输出累计。 */
  outputTokens: UsageMeasurement;
  /** Reasoning totals.
   * 推理累计。 */
  reasoningTokens: UsageMeasurement;
  /** Reported total tokens.
   * 上报的总 Token。 */
  totalTokens: UsageMeasurement;
  /** Budget dimensions paired per request.
   * 按请求配对的预算维度。 */
  budgetTokens: UsageMeasurement;
  /** Dispatch-to-first-token duration sum.
   * 分发至首 Token 耗时之和。 */
  firstTokenMs: UsageMeasurement;
  /** Dispatch-to-terminal duration sum.
   * 分发至终态耗时之和。 */
  requestDurationMs: UsageMeasurement;
  /** Paired output generation duration.
   * 配对的输出生成时长。 */
  outputDurationMs: UsageMeasurement;
  /** Paired uncached plus cached input.
   * 配对的未缓存与缓存输入。 */
  cacheInputTokens: UsageMeasurement;
  /** Output from measured speed samples.
   * 测速样本的输出。 */
  measuredOutputTokens: number;
  /** Cache reads from ratio samples.
   * 命中率样本的缓存读取。 */
  measuredCacheReadTokens: number;
}

/** The root file's disjoint lifetime scopes; no overall UI total exists.
 * 根文件中互不重叠的生命周期范围；不存在界面整体合计。 */
export interface SessionLifetimeUsage {
  /** File schema version.
   * 文件结构版本。 */
  version: number;
  /** Stable root session identity.
   * 稳定根会话身份。 */
  rootSessionId: string;
  /** Monotonic aggregate revision.
   * 单调累计版本。 */
  revision: number;
  /** Last update timestamp.
   * 最后更新时间戳。 */
  updatedAtMs: number;
  /** Root's own counters.
   * 根自身计数。 */
  main: SessionUsageBucket;
  /** All descendants' disjoint own counters.
   * 所有后代互不重叠的自身计数。 */
  childrenTotal: SessionUsageBucket;
  /** Stable child records, retained after completion or closure.
   * 完成或关闭后仍保留的稳定子会话记录。 */
  childrenBySessionId: Record<string, {
    /** Verified direct parent session.
     * 经验证的直接父会话。 */
    parentSessionId: string;
    /** This child's own counters.
     * 当前子会话自身计数。 */
    usage: SessionUsageBucket;
  }>;
}
