import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { TurnStatsTooltipCard } from "./conversation-turn-stats-card";

// TURN_STATS_METADATA mirrors the persisted aggregate and first-class subagent usage-group contract.
// TURN_STATS_METADATA 对应已持久化的总计与一等子代理用量分组契约。
const TURN_STATS_METADATA = {
  turn_stats: {
    turn_kind: "user_turn",
    provider_call_count: 4,
    tool_call_count: 7,
    input_tokens: 286_261,
    output_tokens: 305,
    total_tokens: 286_566,
    cached_input_tokens: 282_624,
    output_tokens_per_second: 8.376,
    usage_groups: [
      {
        kind: "regular",
        provider_call_count: 2,
        tool_call_count: 3,
        input_tokens: 20_000,
        output_tokens: 105,
        total_tokens: 20_105,
        cached_input_tokens: 18_000,
      },
      {
        kind: "subagent",
        provider_call_count: 2,
        tool_call_count: 4,
        input_tokens: 266_261,
        output_tokens: 200,
        total_tokens: 266_461,
        cached_input_tokens: 264_624,
      },
    ],
  },
};

describe("turn statistics hover card", () => {
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
    expect(markup).toContain("平均 8.4 tokens/s");
    expect(markup).toContain("总请求次数");
    expect(markup).toContain("工具调用次数");
    expect(markup).toContain("缓存 Tokens");
    expect(markup).toContain("输入 Tokens");
    expect(markup).toContain("输出 Tokens");
    expect(markup).toContain("合计 Tokens");
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

    expect(markup).toContain("turn-stats-subagent-section");
    expect(markup).toContain("子代理 Tokens");
    expect(markup).toContain("266,261");
    expect(markup).toContain("266,461");
    expect(markup).not.toContain("用量分组");
    expect(markup).not.toContain("常规");
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

    expect(markup).toContain("子代理统计");
    expect(markup).not.toContain("turn-stats-subagent-section");
  });
});
