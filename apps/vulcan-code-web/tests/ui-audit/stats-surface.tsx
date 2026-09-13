import React from "react";
import { I18nProvider } from "../../src/i18n";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "../../src/components/ui/tooltip";
import { TurnStatsTooltipCard } from "../../src/components/conversation/conversation-turn-stats-card";
import { ConversationV1TurnStatsCard } from "../../src/components/conversation/conversation-v1-turn-stats-card";
import type { ConversationEventSummary, StatBucket } from "../../src/service/conversation-event-window-store";
import { projectConversationEventDisplay, projectConversationTranscriptDisplay } from "../../src/service/conversation-event-display-projection";

/** Renders both production statistics surfaces from explicit synthetic accounting fixtures.
 * 使用明确的合成核算样本渲染两条生产统计界面。
 * Takes no props; returns focusable tooltips without RPC, model calls, or persistence.
 * 无属性；返回可聚焦浮层，不调用 RPC、模型或持久化。
 */
export function AuditStatsSurface() {
  // query selects the declared language and accounting fixture for this isolated page.
  // query 为此隔离页面选择已声明的语言与核算样本。
  const query = new URLSearchParams(window.location.search);
  // locale follows the two supported production interface languages.
  // locale 遵循生产界面支持的两种语言。
  const locale = query.get("locale") === "en-US" ? "en-US" : "zh-CN";
  // caseName preserves a missing selection as the explicit measured fixture.
  // caseName 在未选择时使用明确的已测量样本。
  const caseName = query.get("statsCase") ?? "measured";
  if (!["measured", "unknown", "zero"].includes(caseName)) throw new Error(`Unknown stats audit case: ${caseName}`);
  // outcome uses exactly the three persisted V1 closure states.
  // outcome 使用准确的三种 V1 持久化闭合状态。
  const outcome = query.get("outcome") ?? "completed";
  if (outcome !== "completed" && outcome !== "abandoned" && outcome !== "stopped") throw new Error(`Unknown stats audit outcome: ${outcome}`);
  // includeChild adds independently identified child accounting only to the explicit measured scenario.
  // includeChild 仅在明确的已测量场景中添加可独立识别的子代理核算。
  const includeChild = query.get("child") === "1";
  if (includeChild && caseName !== "measured") throw new Error("STATS_AUDIT_CHILD_REQUIRES_MEASURED_CASE");
  // output distinguishes absent usage from an explicit zero and a measured positive count.
  // output 区分缺失用量、明确零值与已测量正值。
  const output = caseName === "unknown" ? undefined : caseName === "zero" ? 0 : 106;
  // metadata supplies canonical normalized counts; diagnostic labels never determine input semantics.
  // metadata 提供权威归一化计数；诊断标签不决定输入语义。
  const metadata = { turn_stats: {
    provider_call_count: 2, tool_call_count: 1, cached_input_tokens: 45952,
    output_tokens: output, total_tokens: caseName === "unknown" ? undefined : 46494 + (output ?? 0),
    output_tokens_per_second: output === undefined ? undefined : output / 2,
    usage_segments: [{ kind: "context_compaction", execution_profile: "pruned_minimal", uncached_input_tokens: caseName === "unknown" ? undefined : 542 }],
    usage_groups: includeChild ? [{ kind: "subagent", provider_call_count: 1, tool_call_count: 0, cached_input_tokens: 100, uncached_input_tokens: 6, output_tokens: 6, total_tokens: 112 }] : [],
  } };
  // direct keeps both inclusive and direct accounting consistent for the synthetic terminal event.
  // direct 保持合成终态事件的总计与直接核算一致。
  const direct: StatBucket = {
    api_call_count: 2, tool_call_count: 1, unknown_usage_api_call_count: caseName === "unknown" ? 2 : 0, subagent_return_count: 0,
    usage: { output_tokens: output, cache_read_tokens: 45952, uncached_input_tokens: caseName === "unknown" ? undefined : 542,
      total_tokens: caseName === "unknown" ? undefined : 46494 + (output ?? 0) },
  };
  // child has its own complete measured bucket, or no facts when the child section is absent.
  // child 有独立的完整测量桶；子代理分区不存在时不包含事实。
  const child: StatBucket = {
    api_call_count: includeChild ? 1 : 0, tool_call_count: 0, unknown_usage_api_call_count: 0, subagent_return_count: includeChild ? 1 : 0,
    usage: includeChild ? { output_tokens: 6, cache_read_tokens: 100, uncached_input_tokens: 6, total_tokens: 112 } : {},
  };
  // total follows V1 inclusive accounting; the legacy parent fixture above deliberately retains its older direct scope.
  // total 遵循 V1 包含式核算；上方旧版父级样本有意保留其旧有直接范围。
  const total: StatBucket = includeChild ? { ...direct, api_call_count: 3, subagent_return_count: 1,
    usage: { output_tokens: 112, cache_read_tokens: 46052, uncached_input_tokens: 548, total_tokens: 46712 },
  } : direct;
  // events exercise the real V1 projection without accessing a production session or copied user record.
  // events 验证真实 V1 投影，不访问生产会话或复制的用户记录。
  const events: ConversationEventSummary[] = [{
    seq: 9, turn_id: 2, event_revision: 1, created_at_ms: 1, updated_at_ms: 1, visibility: "ui_only",
    event: { kind: "turn_stats", start_seq: 1, end_seq: 9, outcome, duration_ms: 2000, buckets: {
      direct, total, subagent: child,
    } },
  }];
  // node resolves only the exact terminal statistics node; invalid fixture projection fails explicitly.
  // node 只解析准确的终态统计节点；样本投影无效时明确失败。
  const node = projectConversationTranscriptDisplay(projectConversationEventDisplay(events)).find((item) => item.kind === "turn_stats");
  if (!node || node.kind !== "turn_stats") throw new Error("STATS_AUDIT_TERMINAL_NODE_MISSING");
  return <I18nProvider initialLocale={locale}><TooltipProvider>
    <main style={{ padding: 16, paddingTop: 280 }}>
      <p>合成统计样本 · {caseName} · 非真实模型调用</p>
      <Tooltip><TooltipTrigger asChild><div className="turn-stats-row completed" tabIndex={0}>分支统计浮层</div></TooltipTrigger>
        <TooltipContent className="context-usage-tooltip turn-stats-tooltip" align="start" side="top" sideOffset={8} showArrow={false}>
          <TurnStatsTooltipCard locale={locale} subagentStats={false} metadata={metadata} />
        </TooltipContent>
      </Tooltip>
      <div style={{ marginTop: 24 }}><ConversationV1TurnStatsCard node={node} /></div>
    </main>
  </TooltipProvider></I18nProvider>;
}
