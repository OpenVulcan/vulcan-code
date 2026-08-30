import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { kimiPartialUsageEvents } from "@/service/__fixtures__/kimi-partial-usage";
import { projectConversationEventDisplay, projectConversationTranscriptDisplay } from "@/service/conversation-event-display-projection";
import { projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import { TurnStatsTooltipCard } from "./conversation-turn-stats-card";
import { ConversationV1TurnStatsCard } from "./conversation-v1-turn-stats-card";

// Render tooltip content in SSR so both real statistics-card bodies can be asserted.
// 在 SSR 中渲染浮层内容，以断言两条真实统计卡的正文。
vi.mock("@/components/ui/tooltip", () => {
  const Passthrough = (props: { children: React.ReactNode }) => <>{props.children}</>;
  return { Tooltip: Passthrough, TooltipContent: Passthrough, TooltipProvider: Passthrough, TooltipTrigger: Passthrough };
});

describe("partial usage display from old persisted V1 events", () => {
  it("shows known counts and the paired known-request ratio in the main branch card", () => {
    const events = kimiPartialUsageEvents();
    const original = JSON.stringify(events);
    const branch = projectConversationBranchPresentationV1("fixture", events, {
      sessionId: "fixture", resources: new Map(), contentUrl: () => "", error: null, status: "ready",
    }, (key) => key);
    const metadata = branch.messages.find((message) => message.recordKind === "turn-stats")!.metadata;
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="zh-CN" subagentStats={false} />);
    expect(markup).toContain("179,456");
    expect(markup).toContain("16,090");
    expect(markup).toContain("缓存 Tokens</span>");
    expect(markup).toContain("无缓存输入</span>");
    expect(markup).toContain("缓存命中率</span>");
    expect(markup).toContain("91.8%");
    expect(markup).not.toMatch(/≥|已知|7\/8|turn-stats-coverage-note/);
    expect(markup.match(/class="turn-stats-metric-row"/g)).toHaveLength(3);
    expect(markup).toContain("214,777");
    expect(markup).not.toContain("83.98");
    expect(JSON.stringify(events)).toBe(original);
  });

  it("uses identical evidence on the V1-native statistics card", () => {
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay(kimiPartialUsageEvents()));
    const node = nodes.find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1TurnStatsCard node={node} /></I18nProvider>);
    expect(markup).toContain("179,456");
    expect(markup).toContain("16,090");
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
    expect(markup).toContain("179,456");
    expect(markup).toContain("Cached tokens</span>");
    expect(markup).toContain("Cache hit rate</span>");
    expect(markup).not.toMatch(/≥|known|Known|7\/8|turn-stats-coverage-note/);
    // Native transcript projection must also collect evidence before visibility filtering.
    // 原生转录投影同样必须在可见性过滤前收集证据。
    const node = projectConversationTranscriptDisplay(projectConversationEventDisplay(events)).find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    expect(node.usage_coverage?.total.dimensions.cache_read_tokens.knownTokens).toBe(179456);
  });

  it("displays empty cache as zero without modifying the old terminal summary", () => {
    const nodes = projectConversationTranscriptDisplay(projectConversationEventDisplay(kimiPartialUsageEvents().slice(-1)));
    const node = nodes.find((candidate) => candidate.kind === "turn_stats");
    if (!node || node.kind !== "turn_stats") throw new Error("expected terminal stats node");
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1TurnStatsCard node={node} /></I18nProvider>);
    expect(markup).toContain("缓存 Tokens</span><strong>0</strong>");
    expect(markup).toContain("无缓存输入</span><strong>0</strong>");
    expect(node.item.event.buckets.total.usage.cache_read_tokens).toBeUndefined();
    expect(markup).toContain("暂无数据");
    expect(markup).not.toContain("91.8%");
    expect(markup).not.toContain("（已知）");
  });

  // Empty values are a display default and must not be persisted as measured zero usage.
  // 空值零是展示默认值，不得持久化为实测零用量。
  it("uses the same empty-value default in the main card without adding rows", () => {
    // A historical summary can omit cache fields even when other dimensions are present.
    // 历史摘要可能缺少缓存字段，同时仍包含其他维度。
    const metadata = { turn_stats: { provider_call_count: 1, tool_call_count: 0, output_tokens: 1, total_tokens: 11 } };
    // Render the actual main-card body so zero formatting and layout are verified together.
    // 渲染真实主卡正文，同时验证零值格式与布局。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard metadata={metadata} locale="zh-CN" subagentStats={false} />);
    expect(markup).toContain("缓存 Tokens</span><strong>0</strong>");
    expect(markup).toContain("无缓存输入</span><strong>0</strong>");
    expect(markup.match(/class="turn-stats-metric-row"/g)).toHaveLength(3);
    expect(metadata.turn_stats).not.toHaveProperty("cached_input_tokens");
  });
});
