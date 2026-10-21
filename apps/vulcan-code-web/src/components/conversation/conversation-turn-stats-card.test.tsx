import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { readLocaleMessage } from "@/i18n";

import { TurnStatsTooltipCard } from "./conversation-turn-stats-card";

// TURN_STATS_METADATA mirrors the persisted aggregate and first-class subagent usage-group contract.
// TURN_STATS_METADATA 对应已持久化的总计与一等子代理用量分组契约。
const TURN_STATS_METADATA = {
  turn_stats: {
    turn_kind: "user_turn",
    provider_call_count: 4,
    tool_call_count: 7,
    input_tokens: 286_261,
    uncached_input_tokens: 3_637,
    output_tokens: 305,
    total_tokens: 286_566,
    cached_input_tokens: 282_624,
    output_tokens_per_second: 8.376,
    output_duration_ms: 36_414,
    time_to_first_token_ms: 750,
    timed_api_call_count: 2,
    usage_groups: [
      {
        kind: "regular",
        provider_call_count: 2,
        tool_call_count: 3,
        input_tokens: 20_000,
        uncached_input_tokens: 2_000,
        output_tokens: 105,
        total_tokens: 20_105,
        cached_input_tokens: 18_000,
      },
      {
        kind: "subagent",
        provider_call_count: 2,
        tool_call_count: 4,
        input_tokens: 266_261,
        uncached_input_tokens: 1_637,
        output_tokens: 200,
        total_tokens: 266_461,
        cached_input_tokens: 264_624,
      },
    ],
  },
};

describe("turn statistics hover card", () => {
  // A compaction launch profile does not identify wire-protocol input-token semantics.
  // 压缩启动策略不代表线协议的输入 Token 语义。
  it("keeps missing normalized input unknown for a real compaction strategy label", () => {
    // markup uses the persisted strategy from context_compaction_usage_segment_from_execution.
    // markup 使用 context_compaction_usage_segment_from_execution 持久化的真实策略标签。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard locale="zh-CN" subagentStats={false} metadata={{
      turn_stats: {
        provider_call_count: 1, tool_call_count: 0, input_tokens: 120, output_tokens: 5,
        total_tokens: 205, cached_input_tokens: 80,
        usage_segments: [{ kind: "context_compaction", execution_profile: "pruned_minimal", input_tokens: 120, cached_input_tokens: 80 }],
      },
    }} />);
    expect(markup).toContain("无缓存输入</span><strong>-</strong>");
    expect(markup).not.toContain("aria-valuenow=");
  });

  // Verifies the card replaces raw debug text with the requested three-row aggregate layout and cache bar.
  // 验证卡片以所需三行总计布局和缓存进度条替代原始调试文本。
  it("renders the requested aggregate fields and cache-hit progress", () => {
    const markup = renderToStaticMarkup(
      <TurnStatsTooltipCard
        locale="zh-CN"
        metadata={TURN_STATS_METADATA}
        subagentStats={false}
      />,
    );

    expect(markup).toContain("本轮统计");
    expect(markup).toContain("平均 8.4 Token/s");
    expect(markup).toContain("总请求次数");
    expect(markup).toContain("工具调用次数");
    expect(markup).toContain("缓存 Token");
    expect(markup).toContain("无缓存输入");
    expect(markup).toContain("<strong>3.6K</strong>");
    expect(markup).not.toContain("286.3K");
    expect(markup).toContain("输出 Token");
    expect(markup).toContain("合计 Token");
    expect(markup).toContain("98.7%");
    expect(markup).toContain('role="progressbar"');
  });

  // Verifies regular grouping is omitted while child-agent accounting gets one separated three-row section.
  // 验证常规用量分组被省略，而子代理核算获得独立的三行分区。
  it("renders only the first-class subagent section below the aggregate totals", () => {
    const markup = renderToStaticMarkup(
      <TurnStatsTooltipCard
        locale="zh-CN"
        metadata={TURN_STATS_METADATA}
        subagentStats={false}
      />,
    );

    expect(markup).toContain("data-turn-stats-subagent");
    expect(markup).toContain("智能体 Token");
    expect(markup).toContain("<strong>1.6K</strong>");
    expect(markup).toContain("<strong>266.5K</strong>");
    expect(markup).not.toContain("266.3K");
    expect(markup).not.toContain("用量分组");
    expect(markup).not.toContain("常规");
  });

  // Verifies normalized parent segments are summed without inferring input semantics from diagnostic labels.
  // 验证归一化父级分段会被累加，而不会根据诊断标签推断输入语义。
  it("sums normalized parent segments and excludes detached child usage", () => {
    // markup contains both parent segment kinds and a child segment with independent accounting.
    // markup 包含两种父级分段与具有独立核算的子代理分段。
    const markup = renderToStaticMarkup(
      <TurnStatsTooltipCard
        locale="zh-CN"
        metadata={{
          turn_stats: {
            turn_kind: "user_turn",
            provider_call_count: 1,
            tool_call_count: 0,
            input_tokens: 12,
            output_tokens: 5,
            total_tokens: 105,
            cached_input_tokens: 80,
            cache_creation_input_tokens: 8,
            usage_segments: [{
              kind: "assistant_response",
              provider_call_count: 1,
              input_tokens: 12,
              uncached_input_tokens: 12,
              output_tokens: 5,
              total_tokens: 105,
              cached_input_tokens: 80,
              cache_creation_input_tokens: 8,
            }, {
              kind: "context_compaction",
              execution_profile: "pruned_minimal",
              uncached_input_tokens: 8,
            }, {
              kind: "subagent_task",
              uncached_input_tokens: 900,
            }],
          },
        }}
        subagentStats={false}
      />,
    );

    expect(markup).toContain("无缓存输入");
    expect(markup).toContain(">20<");
    expect(markup).toContain("80.0%");
  });

  // Verifies missing or invalid parent segments never produce a misleading partial cache ratio.
  // 验证缺失或无效的父级分段不会生成具有误导性的局部缓存比例。
  it.each([
    [{ kind: "assistant_response", uncached_input_tokens: 12 }, { kind: "context_compaction", input_tokens: 8 }],
    [{ kind: "assistant_response", execution_profile: "anthropic-messages.v1", input_tokens: 12 }],
    [{ kind: "assistant_response", uncached_input_tokens: -1 }],
    [{ kind: "future_kind", uncached_input_tokens: 12 }],
    [{ kind: "subagent_task", uncached_input_tokens: 12 }],
    [null],
    [],
  ].map((segments) => [segments]))("preserves unknown usage for incomplete segment set %#", (segments) => {
    // markup exercises the rendered unknown value and absence of a quantitative progress bar.
    // markup 验证渲染的未知值及定量进度条的缺失。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard locale="zh-CN" subagentStats={false} metadata={{
      turn_stats: { cached_input_tokens: 80, usage_segments: segments },
    }} />);
    expect(markup).toContain("无缓存输入</span><strong>-</strong>");
    expect(markup).not.toContain("aria-valuenow=");
  });

  // Verifies explicit zero remains a measurement and the persisted aggregate wins over segment reconstruction.
  // 验证明确的零仍是测量值，且持久化总计优先于分段重建。
  it.each([
    { cached_input_tokens: 80, usage_segments: [{ kind: "assistant_response", uncached_input_tokens: 0 }] },
    { cached_input_tokens: 80, uncached_input_tokens: 0, usage_segments: [{ kind: "assistant_response", uncached_input_tokens: 12 }] },
  ])("retains measured zero and authoritative aggregate %#", (stats) => {
    // markup must present a complete cache hit without erasing a measured zero.
    // markup 必须呈现全部命中缓存，且不能抹除已测量的零值。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard locale="zh-CN" subagentStats={false} metadata={{ turn_stats: stats }} />);
    expect(markup).toContain("无缓存输入</span><strong>0</strong>");
    expect(markup).toContain('aria-valuenow="100"');
  });

  // Verifies an independent subagent-feedback row does not duplicate a nested subagent section.
  // 验证独立子代理反馈行不会重复嵌套子代理分区。
  it("does not duplicate the child section for a child-only statistics row", () => {
    const markup = renderToStaticMarkup(
      <TurnStatsTooltipCard
        locale="zh-CN"
        metadata={TURN_STATS_METADATA}
        subagentStats
      />,
    );

    expect(markup).toContain(readLocaleMessage("zh-CN", "conversation.turnStats.subagentUsage"));
    expect(markup).not.toContain("data-turn-stats-subagent");
  });

  // Child feedback owns its child segments, unlike the same segments grouped under a parent user turn.
  // 子代理反馈拥有自身的子代理分段，与父级用户轮次下的同类分组不同。
  it("includes normalized child segments in a standalone feedback row", () => {
    // markup follows the subagent_feedback_turn_stats producer's inclusive merge contract.
    // markup 遵循 subagent_feedback_turn_stats 产出方的包含式合并契约。
    const markup = renderToStaticMarkup(<TurnStatsTooltipCard locale="zh-CN" subagentStats metadata={{ turn_stats: {
      turn_kind: "subagent_feedback", cached_input_tokens: 80,
      usage_segments: [{ kind: "subagent_task", uncached_input_tokens: 20 }],
    } }} />);
    expect(markup).toContain("无缓存输入</span><strong>20</strong>");
    expect(markup).toContain('aria-valuenow="80"');
    expect(markup).not.toContain("data-turn-stats-subagent");
  });
});
