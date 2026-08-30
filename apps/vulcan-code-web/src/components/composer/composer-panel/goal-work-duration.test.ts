import { describe, expect, it } from "vitest";
import type { SessionGoalSummary } from "@/types";
import { formatGoalWorkDuration, goalElapsedMilliseconds } from "./goal-work-duration";

// goal supplies a precise persisted fractional checkpoint without any network clock.
// goal 提供精确的持久化零碎毫秒检查点，不使用网络时钟。
const goal: SessionGoalSummary = {
  sessionId: "session", goalId: "goal", objective: "work", status: "active",
  tokensUsed: 0, usageTokens: 0, apiCallCount: 0, toolCallCount: 0, inputTokens: 0,
  cachedInputTokens: 0, outputTokens: 0, reasoningTokens: 0, totalTokens: 0,
  timeUsedSeconds: 2, workTime: { elapsedMs: 2_650, observedAtMs: 10_000 },
  createdAtMs: 1_000, updatedAtMs: 10_000,
};

describe("goal working clock", () => {
  it("interpolates active time from the backend observation, not arrival time", () => {
    expect(goalElapsedMilliseconds(goal, 11_500)).toBe(4_150);
    expect(goalElapsedMilliseconds(goal, 9_000)).toBe(2_650);
  });
  it.each(["paused", "blocked", "usage_limited", "budget_limited", "complete"] as const)("does not count downtime in %s", (status) => {
    expect(goalElapsedMilliseconds({ ...goal, status }, 10_000_000)).toBe(2_650);
  });
  it("does not invent a running clock for a draft or old snapshot", () => {
    expect(goalElapsedMilliseconds({ ...goal, workTime: undefined }, 10_000_000)).toBe(2_000);
  });
  it("keeps total hours and fractional checkpoints without resetting on resumption", () => {
    expect(formatGoalWorkDuration(26 * 3_600_000 + 61_900)).toBe("26:01:01");
    expect(formatGoalWorkDuration(999)).toBe("0:00:00");
    expect(formatGoalWorkDuration(3_600_000)).toBe("1:00:00");
  });
});
