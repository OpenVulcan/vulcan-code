import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nProvider } from "@/i18n";
import type { SessionLifetimeUsage, SessionUsageBucket } from "@/types/session-lifetime-usage";
import { ChildLifetimeUsageChip, LifetimeUsageBucket, SessionLifetimeUsagePanel, lifetimeTotal } from "./session-lifetime-usage";

/** Builds a deterministic aggregate fixture with independently counted measurement samples.
 * 构造测量样本独立计数的确定性累计测试数据。 */
function bucket(tokens: number): SessionUsageBucket {
  return {
    requestCount: 2, unknownUsageCount: 0, toolCallCount: 3,
    inputTokens: { sum: 100, samples: 2 }, uncachedInputTokens: { sum: 40, samples: 2 },
    outputTokens: { sum: 60, samples: 2 }, reasoningTokens: { sum: 0, samples: 0 },
    cacheReadTokens: { sum: 60, samples: 2 }, cacheWriteTokens: { sum: 0, samples: 2 },
    totalTokens: { sum: tokens, samples: 2 }, budgetTokens: { sum: 100, samples: 2 },
    firstTokenMs: { sum: 600, samples: 1 }, requestDurationMs: { sum: 1600, samples: 1 },
    outputDurationMs: { sum: 1000, samples: 1 }, measuredOutputTokens: 20,
    cacheInputTokens: { sum: 100, samples: 2 }, measuredCacheReadTokens: 60,
  };
}

describe("session lifetime usage", () => {
  it("keeps scopes separate and counts stable child sessions without an overall total", () => {
    const usage: SessionLifetimeUsage = {
      version: 1, rootSessionId: "root", revision: 3, updatedAtMs: 1,
      main: bucket(111), childrenTotal: bucket(222), legacyIncomplete: false,
      childrenBySessionId: { child: { parentSessionId: "root", usage: bucket(222) } },
    };
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><SessionLifetimeUsagePanel usage={usage} /></I18nProvider>);
    expect(markup).toContain("111");
    expect(markup).toContain("222");
    expect(markup).not.toContain("333");
    expect(markup).toContain("Main session usage");
    expect(markup).toContain("Subagent usage · 1 agents");
  });

  it("uses reporting samples for latency and paired output tokens for speed", () => {
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><LifetimeUsageBucket title="own" bucket={bucket(160)} /></I18nProvider>);
    expect(markup).toContain("0.600 s");
    expect(markup).toContain("20.0 t/s");
    expect(markup).toContain("60.0%");
    expect(markup).toContain("first token 1");
  });

  it("preserves zero, missing and partial dimensions and exposes an own-child chip", () => {
    expect(lifetimeTotal({ sum: 0, samples: 0 }, 0)).toBe("0");
    expect(lifetimeTotal({ sum: 0, samples: 0 }, 2)).toBe("—");
    expect(lifetimeTotal({ sum: 0, samples: 2 }, 2)).toBe("0");
    expect(lifetimeTotal({ sum: 12, samples: 1 }, 2)).toBe("≥ 12");
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="en-US"><ChildLifetimeUsageChip bucket={bucket(77)} /></I18nProvider>);
    expect(markup).toContain("Child lifetime usage");
    expect(markup).toContain("77");
    expect(markup).toContain("button");
  });
});
