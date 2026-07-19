import {
  applyStreamingMarkdownDeltas,
  createStreamingMarkdownRenderState,
  type StreamingMarkdownRenderState,
} from "../src/components/conversation/streaming-markdown-render-session";
import type {
  ConversationLiveStreamBodySnapshot,
  ConversationLiveStreamRevision,
} from "../src/state/conversation-live-stream-store";
import { describe, expect, it } from "vitest";

// BENCHMARK_SIZES_KIB defines the historical body sizes compared with the original cumulative renderer baseline.
// BENCHMARK_SIZES_KIB 定义与原累计渲染器基线对比的历史正文大小。
const BENCHMARK_SIZES_KIB = [8, 32, 64, 128] as const;

// BENCHMARK_REPETITIONS smooths timer noise without making the local verification slow.
// BENCHMARK_REPETITIONS 平滑计时噪声，同时避免本地验证耗时过长。
const BENCHMARK_REPETITIONS = 20;

// ONE_KIB_SAFE_BLOCK is one independently safe Markdown paragraph delivered per update.
// ONE_KIB_SAFE_BLOCK 是每次更新投递的一个可独立解析 Markdown 段落。
const ONE_KIB_SAFE_BLOCK = `${"x".repeat(1022)}\n\n`;

// benchmarkBodySnapshot builds the empty atomic Store snapshot used at benchmark cutover.
// benchmarkBodySnapshot 构建基准切入点使用的空原子 Store 快照。
//
// Returns: one active stream snapshot at generation one.
// 返回值：generation 为一的活跃流快照。
function benchmarkBodySnapshot(): ConversationLiveStreamBodySnapshot {
  return {
    key: "benchmark\u0000message",
    conversationId: "benchmark",
    messageId: "message",
    recordKind: "assistant-output",
    lifecycle: "active",
    baseGeneration: 1,
    lastGeneration: 1,
    version: 1,
    utf16Length: 0,
    utf8ByteLength: 0,
    body: "",
  };
}

// benchmarkRevision builds one active Store revision after an exact update count.
// benchmarkRevision 构建精确更新次数后的活跃 Store revision。
//
// Parameters: generation is the latest backend event consumed by the render session.
// 参数：generation 是渲染会话消费的最新后端事件。
// Returns: one immutable active revision.
// 返回值：一份不可变活跃 revision。
function benchmarkRevision(generation: number): ConversationLiveStreamRevision {
  return {
    ...benchmarkBodySnapshot(),
    baseGeneration: 1,
    lastGeneration: generation,
    version: generation,
  };
}

// runBenchmarkSample measures incremental scanner and segmentation work for one final body size.
// runBenchmarkSample 测量一个最终正文大小对应的增量扫描与分段工作。
//
// Parameters: sizeKiB is both the final KiB size and the number of 1KiB updates.
// 参数：sizeKiB 同时表示最终 KiB 大小与 1KiB 更新次数。
// Returns: elapsed milliseconds and final render state.
// 返回值：耗时毫秒数与最终渲染状态。
function runBenchmarkSample(sizeKiB: number): { elapsedMs: number; state: StreamingMarkdownRenderState } {
  let state = createStreamingMarkdownRenderState(benchmarkBodySnapshot());
  const startedAt = performance.now();
  for (let index = 0; index < sizeKiB; index += 1) {
    const generation = index + 2;
    state = applyStreamingMarkdownDeltas(
      state,
      [{ generation, text: ONE_KIB_SAFE_BLOCK }],
      benchmarkRevision(generation),
    );
  }
  return { elapsedMs: performance.now() - startedAt, state };
}

// median returns the stable center measurement from an ordered numeric sample.
// median 返回有序数值样本的稳定中位测量值。
//
// Parameters: values contains at least one elapsed-time measurement.
// 参数：values 至少包含一个耗时测量值。
// Returns: the median elapsed time.
// 返回值：耗时中位数。
function median(values: readonly number[]): number {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.floor(ordered.length / 2)] ?? 0;
}

describe("streaming Markdown performance benchmark", () => {
  it("records bounded-tail scaling from 8KB through 128KB", () => {
    for (const sizeKiB of BENCHMARK_SIZES_KIB) {
      const samples = Array.from(
        { length: BENCHMARK_REPETITIONS },
        () => runBenchmarkSample(sizeKiB),
      );
      const finalState = samples.at(-1)?.state;
      const elapsedMedianMs = median(samples.map((sample) => sample.elapsedMs));
      console.info("streaming-markdown-benchmark", JSON.stringify({
        sizeKiB,
        updateCount: sizeKiB,
        elapsedMedianMs: Number(elapsedMedianMs.toFixed(3)),
        averageUpdateMs: Number((elapsedMedianMs / sizeKiB).toFixed(4)),
        committedSegments: finalState?.committedSegments.length ?? 0,
        activeMarkdownTailBytes: finalState?.tailUtf8ByteLength ?? 0,
      }));
      expect(finalState?.tailUtf8ByteLength ?? 0).toBeLessThanOrEqual(1024);
      expect(finalState?.committedSegments.length ?? 0).toBe(sizeKiB / 2);
    }
  });
});
