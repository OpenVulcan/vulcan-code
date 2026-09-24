import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { kimiPartialUsageEvents } from "@/service/__fixtures__/kimi-partial-usage";
import { projectConversationEventDisplay, projectConversationTranscriptDisplay } from "@/service/conversation-event-display-projection";
import { projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import { TurnStatsTooltipCard } from "./conversation-turn-stats-card";
import { ConversationV1TurnStatsCard } from "./conversation-v1-turn-stats-card";
import { averageTurnOutputSpeed, usageMetricDisplay } from "./conversation-usage-display";

// Render tooltip content in SSR so both real statistics-card bodies can be asserted.
// 在 SSR 中渲染浮层内容，以断言两条真实统计卡的正文。
vi.mock("@/components/ui/tooltip", () => {
  const Passthrough = (props: { children: React.ReactNode }) => <>{props.children}</>;
  return { Tooltip: Passthrough, TooltipContent: Passthrough, TooltipProvider: Passthrough, TooltipTrigger: Passthrough };
});

describe("partial usage display from old persisted V1 events", () => {
  // Throughput requires paired output measurements, never turn duration or a zero divisor.
  // 吞吐率要求配对的输出测量，绝不使用整轮时长或零分母。
  it("keeps missing timings unknown and computes paired output throughput", () => {
    expect(averageTurnOutputSpeed(106, 2000)).toBe(53);
    expect(averageTurnOutputSpeed(0, 2000)).toBe(0);
    expect(averageTurnOutputSpeed(106, 0)).toBeNull();
    expect(averageTurnOutputSpeed(106, undefined)).toBeNull();
    expect(averageTurnOutputSpeed(undefined, 2000)).toBeNull();
    expect(averageTurnOutputSpeed(106, -1)).toBeNull();
    expect(averageTurnOutputSpeed(Infinity, 2000)).toBeNull();
  });

  // Both UI paths must expose the same measured speed after projection from durable V1 facts.
  // 两条界面路径必须在可靠 V1 事实投影后展示相同测得速度。
  it("does not substitute turn duration for missing physical first-token timing", () => {
    const events = kimiPartialUsageEvents().map((summary) => summary.event.kind === "turn_stats" ? {
      ...summary, event: { ...summary.event, duration_ms: 2000, buckets: { ...summary.event.buckets, total: { ...summary.event.buckets.total, usage: { ...summary.event.buckets.total.usage, output_tokens: 106 } } } },
    } : summary);
    const branch = projectConversationBranchPresentationV1("fixture", events, {
      sessionId: "fixture", resources: new Map(), contentUrl: () => "", error: null, status: "ready",
    }, (key) => key);
    const metadata = branch.messages.find((message) => message.recordKind === "turn-stats")!.metadata;
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="zh-CN" subagentStats={false} />);
    expect(markup).toContain("平均 -");
    expect(markup).not.toContain("53.0 tokens/s");
    expect(markup).toContain("缺少配对的首 Token 时间与输出用量");
  });

  // An absent measurement and an explicitly reported zero have different user-visible meanings.
  // 缺失测量与明确上报零值具有不同的用户可见含义。
  it("keeps reported zero distinct from missing usage", () => {
    expect(usageMetricDisplay(0, "cache_read_tokens", null)).toBe("0");
    expect(usageMetricDisplay(null, "cache_read_tokens", null)).toBe("-");
    expect(usageMetricDisplay(undefined, "cache_read_tokens", undefined)).toBe("-");
  });
  it("shows known counts and the paired known-request ratio in the main branch card", () => {
    const events = kimiPartialUsageEvents();
    const original = JSON.stringify(events);
    const branch = projectConversationBranchPresentationV1("fixture", events, {
      sessionId: "fixture", resources: new Map(), contentUrl: () => "", error: null, status: "ready",
    }, (key) => key);
    const metadata = branch.messages.find((message) => message.recordKind === "turn-stats")!.metadata;
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="zh-CN" subagentStats={false} />);
    expect(markup).toContain("179.5K");
    expect(markup).toContain("16.1K");
    expect(markup).toContain("缓存 Tokens</span>");
    expect(markup).toContain("无缓存输入</span>");
    expect(markup).toContain("缓存命中率</span>");
    expect(markup).toContain("91.8%");
    expect(markup).not.toMatch(/≥|已知|7\/8|turn-stats-coverage-note/);
    expect(markup.match(/class="turn-stats-metric-row"/g)).toHaveLength(3);
    expect(markup).toContain("214.8K");
    expect(markup).not.toContain("83.98");
    expect(JSON.stringify(events)).toBe(original);
  });

  it("uses identical evidence on the V1-native statistics card", () => {
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay(kimiPartialUsageEvents()));
    const node = nodes.find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1TurnStatsCard node={node} /></I18nProvider>);
    expect(markup).toContain("179.5K");
    expect(markup).toContain("16.1K");
    expect(markup).not.toMatch(/≥|已知|7\/8|turn-stats-coverage-note/);
    expect(markup).toContain("缓存命中率</span>");
    expect(markup).toContain("91.8%");
  });

  // Hidden accounting metadata remains available to both adapters without rendering a separate API row.
  // 隐藏核算元数据仍供两条适配路径使用，不额外渲染 API 行。
  it("retains host-only usage facts with concise English labels", () => {
    // Mark the API samples as non-visible while leaving the terminal card visible.
    // 将 API 样本标记为不可见，同时保留可见的终态卡。
    const events = kimiPartialUsageEvents();
    for (const summary of events) if (summary.event.kind === "api_stats") summary.visibility = "host_only";
    // Exercise the same legacy-metadata adapter used by the main conversation surface.
    // 验证主会话界面使用的同一条旧元数据适配路径。
    const branch = projectConversationBranchPresentationV1("fixture", events, {
      sessionId: "fixture", resources: new Map(), contentUrl: () => "", error: null, status: "ready",
    }, (key) => key);
    // Select the authoritative terminal message, not text scraped from an assistant response.
    // 选择权威终态消息，不从 assistant 回复中抓取文本。
    const metadata = branch.messages.find((message) => message.recordKind === "turn-stats")!.metadata;
    // The English surface keeps the same amounts without coverage qualifiers.
    // 英文界面保留相同数值，不附加覆盖限定说明。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="en-US" subagentStats={false} />);
    expect(markup).toContain("179.5K");
    expect(markup).toContain("Cached tokens</span>");
    expect(markup).toContain("Cache hit rate</span>");
    expect(markup).not.toMatch(/≥|known|Known|7\/8|turn-stats-coverage-note/);
    // Native transcript projection must also collect evidence before visibility filtering.
    // 原生转录投影同样必须在可见性过滤前收集证据。
    const node = projectConversationTranscriptDisplay(projectConversationEventDisplay(events)).find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    expect(node.usage_coverage?.total.dimensions.cache_read_tokens.knownTokens).toBe(179456);
  });

  it("displays absent cache as a dash without modifying the old terminal summary", () => {
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay(kimiPartialUsageEvents().slice(-1)));
    const node = nodes.find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1TurnStatsCard node={node} /></I18nProvider>);
    expect(markup).toContain("缓存 Tokens</span><strong>-</strong>");
    expect(markup).toContain("无缓存输入</span><strong>-</strong>");
    expect(node.item.event.buckets.total.usage.cache_read_tokens).toBeUndefined();
    expect(markup).toContain("尚无完整输入用量，无法计算缓存命中率");
    expect(markup).not.toContain("91.8%");
    expect(markup).not.toContain("（已知）");
  });

  // Missing values remain distinct from a measured zero, without changing persisted accounting.
  // 缺失值与实测零保持不同，不改变持久化核算。
  it("uses the same empty-value default in the main card without adding rows", () => {
    // A historical summary can omit cache fields even when other dimensions are present.
    // 历史摘要可能缺少缓存字段，同时仍包含其他维度。
    const metadata = { turn_stats: { provider_call_count: 1, tool_call_count: 0, output_tokens: 1, total_tokens: 11 } };
    // Render the actual main-card body so missing-value formatting and layout are verified together.
    // 渲染真实主卡正文，同时验证缺失值格式与布局。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="zh-CN" subagentStats={false} />);
    expect(markup).toContain("缓存 Tokens</span><strong>-</strong>");
    expect(markup).toContain("无缓存输入</span><strong>-</strong>");
    expect(markup.match(/class="turn-stats-metric-row"/g)).toHaveLength(3);
    expect(metadata.turn_stats).not.toHaveProperty("cached_input_tokens");
  });
});
