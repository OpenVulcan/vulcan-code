import { describe, expect, it } from "vitest";
import { measuredProviderOutput } from "./provider-output-timing";

// Timing fixtures distinguish preparation, transport response and actual output checkpoints.
// 计时样本区分准备、传输响应及实际输出时间点。
describe("physical provider output timing", () => {
  it("excludes preparation and first-token latency from the output interval", () => {
    expect(measuredProviderOutput({ prepared_at_ms: 0, dispatched_at_ms: 1000,
      first_event_at_ms: 1200, first_token_at_ms: 4000, completed_at_ms: 6000 }, 100)).toEqual({
      requestCount: 1, requestDurationMs: 5000, firstTokenLatencyMs: 3000,
      outputDurationMs: 2000, outputTokens: 100,
    });
  });
  it("does not use a response-start event as the first token", () => {
    expect(measuredProviderOutput({ dispatched_at_ms: 1000, first_event_at_ms: 1500, completed_at_ms: 6000 }, 100)).toBeNull();
  });
  it("rejects unknown output and zero or inverted generation intervals", () => {
    expect(measuredProviderOutput({ dispatched_at_ms: 1000, first_token_at_ms: 2000, completed_at_ms: 3000 }, undefined)).toBeNull();
    expect(measuredProviderOutput({ dispatched_at_ms: 1000, first_token_at_ms: 2000, completed_at_ms: 2000 }, 100)).toBeNull();
    expect(measuredProviderOutput({ dispatched_at_ms: 2000, first_token_at_ms: 1000, completed_at_ms: 3000 }, 100)).toBeNull();
  });
});
