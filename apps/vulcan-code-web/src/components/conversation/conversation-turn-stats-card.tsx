import type { ReactNode } from "react";

import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
  objectValue,
} from "@/components/conversation/conversation-metadata";
import { readUsageCoverage, type UsageCoverage } from "@/service/conversation-usage-coverage";
import { formatDurationSeconds, formatTokenSpeed } from "@/lib/usage-format";
import { usageCacheRatio, usageMetricDisplay } from "./conversation-usage-display";
import { formatMessage, readLocaleMessage, type LocaleCode, type Translate } from "@/i18n";

// TurnStatsUsageSnapshot contains only the accounting fields shown in the compact statistics card.
// TurnStatsUsageSnapshot 仅包含紧凑统计卡展示的核算字段。
interface TurnStatsUsageSnapshot {
  // Number of Provider requests represented by this scope.
  // 当前范围包含的 Provider 请求次数。
  providerCallCount: number;
  // Number of tool calls represented by this scope.
  // 当前范围包含的工具调用次数。
  toolCallCount: number;
  // Protocol-normalized non-cached input token count.
  // 经过协议归一化的无缓存输入 Token 数。
  uncachedInputTokens: number | null;
  // Provider-reported output token count.
  // Provider 报告的输出 Token 数。
  outputTokens: number | null;
  // Provider-reported total token count.
  // Provider 报告的总 Token 数。
  totalTokens: number | null;
  // Optional Provider-reported cache-hit input token count.
  // Provider 报告的可选缓存命中输入 Token 数。
  cachedInputTokens: number | null;
  // Coverage is derived only for V1 views and never replaces the strict persisted totals.
  // 覆盖信息仅为 V1 视图派生，绝不替换严格持久化总量。
  coverage: UsageCoverage | null;
}

// TurnStatsTooltipCard renders paired usage and timing rows, a cache-hit bar, and an optional child-agent section.
// TurnStatsTooltipCard 渲染双列用量与计时数据、缓存命中进度条及可选子代理分区。
//
// Parameters:
// 参数：metadata 是持久化统计载荷，locale 是界面语言，subagentStats 标识纯子代理统计行。
// Returns the structured statistics card, or null when no authoritative turn_stats object exists.
// 返回结构化统计卡；不存在权威 turn_stats 对象时返回 null。
export function TurnStatsTooltipCard(props: {
  // Persisted metadata attached to the turn statistics block.
  // 附加在轮次统计块上的持久化元数据。
  metadata: unknown;
  // Active interface locale.
  // 当前界面语言。
  locale: LocaleCode;
  // Whether this card already represents child-agent-only accounting.
  // 当前卡片是否已经仅代表子代理核算。
  subagentStats: boolean;
}) {
  // stats is the sole authoritative structured source for the card.
  // stats 是本卡片唯一的权威结构化数据源。
  const stats = turnStatsMetadataObject(props.metadata);
  if (!stats) {
    return null;
  }
  // t resolves the card's compact labels in the locale the caller passed.
  // t 按调用者传入的语言解析卡片的紧凑标签。
  const t: Translate = (key) => readLocaleMessage(props.locale, key);
  // aggregate keeps exact persisted totals separate from optional view-only coverage.
  // aggregate 将精确持久化总量与可选的纯视图覆盖信息分开保留。
  const aggregate = turnStatsUsageSnapshot(stats);
  // subagent contains the first-class child-agent group only for a parent turn.
  // subagent 仅在父轮次中包含一等子代理分组。
  const subagent = props.subagentStats ? null : turnStatsSubagentUsageSnapshot(stats);
  // cacheRatio uses exact input components or paired evidence from the same known requests.
  // cacheRatio 使用精确输入分项，或同一批已知请求的配对证据。
  const cacheRatio = usageCacheRatio(aggregate.cachedInputTokens, aggregate.uncachedInputTokens, aggregate.coverage);
  // A null percentage means no complete pair with a positive input denominator is available.
  // 百分比为 null 表示没有分母为正的完整输入配对。
  const cacheHitPercent = cacheRatio.percent;
  // Keep the original short label without exposing internal coverage metadata.
  // 保持原有简短标签，不展示内部覆盖元数据。
  const cacheLabel = t("conversation.turnStats.cacheHitRate");
  // Only the new paired timing contract can establish speed; historical turn-duration estimates are ignored.
  // 仅新的配对计时契约能够证明速度；忽略历史整轮耗时估算。
  const outputDurationMs = numericMetadata(stats.output_duration_ms);
  const firstTokenLatencyMs = numericMetadata(stats.time_to_first_token_ms);
  const speed = outputDurationMs !== null && outputDurationMs > 0 ? numericMetadata(stats.output_tokens_per_second) : null;
  // speedLabel keeps the average speed compact in the card header.
  // speedLabel 在卡片标题中紧凑展示平均速度。
  const speedLabel = speed != null && Number.isFinite(speed) && speed >= 0
    ? formatTokenSpeed(speed)
    : "-";

  return (
    <>
      <div className="vk-sum-title">
        <strong>{props.subagentStats ? t("conversation.turnStats.subagentUsage") : t("conversation.turnStats.turnUsage")}</strong>
        <span title={speedLabel === "-" ? t("conversation.turnStats.pairedFirstTokenTiming") : t("conversation.turnStats.outputTokensDividedBy")}>{formatMessage(t("conversation.turnStats.average"), { speedLabel })}</span>
      </div>
      <TurnStatsUsageRows locale={props.locale} usage={aggregate}>
        {outputDurationMs !== null && outputDurationMs > 0 ? <>
          <div className="vk-stats-pair">
            {firstTokenLatencyMs !== null ? <div className="vk-stat"><span>{t("conversation.turnStats.averageFirstToken")}</span><strong>{formatDurationSeconds(firstTokenLatencyMs)}</strong></div> : null}
            <div className="vk-stat"><span>{t("conversation.turnStats.totalOutputTime")}</span><strong>{formatDurationSeconds(outputDurationMs)}</strong></div>
          </div>
        </> : null}
      </TurnStatsUsageRows>
      <div className="vk-stats-grid">
        <div className="vk-stat">
          <span>{cacheLabel}</span>
          <strong title={cacheHitPercent == null ? t("conversation.turnStats.cacheHitRateNeeds") : undefined}>{cacheHitPercent == null ? "-" : `${cacheHitPercent.toFixed(1)}%`}</strong>
        </div>
        <div
          aria-label={cacheLabel}
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={cacheHitPercent ?? undefined}
          className="vk-stats-bar"
          role="progressbar"
        >
          <span style={{ width: `${cacheHitPercent ?? 0}%` }} />
        </div>
      </div>
      {subagent ? (
        <section className="vk-stats-section" data-turn-stats-subagent>
          <h4>{t("conversation.turnStats.subagentTokens")}</h4>
          <TurnStatsUsageRows locale={props.locale} usage={subagent} />
        </section>
      ) : null}
    </>
  );
}

// TurnStatsUsageRows renders aligned accounting rows and optional timing rows for one scope.
// TurnStatsUsageRows 为一个统计范围渲染对齐的核算行及可选计时行。
//
// Parameters:
// 参数：usage 包含用量核算字段，locale 选择显示格式，children 提供可选计时行。
// Returns a shared two-column statistics grid.
// 返回统一的双列统计网格。
function TurnStatsUsageRows(props: {
  // Usage values for the current card section.
  // 当前卡片分区的用量值。
  usage: TurnStatsUsageSnapshot;
  // Active interface locale.
  // 当前界面语言。
  locale: LocaleCode;
  // Additional measured timing rows share the accounting grid's spacing and typography.
  // 附加的已测计时行复用核算网格的间距与字体样式。
  children?: ReactNode;
}) {
  // t resolves the compact labels used in each paired row in the caller's locale.
  // t 按调用者语言解析每行双列使用的紧凑标签。
  const t: Translate = (key) => readLocaleMessage(props.locale, key);
  // cached displays recorded counts and preserves absence with a dash.
  // cached 显示已记录数值，并以短横线保留缺失状态。
  const cached = usageMetricDisplay(props.usage.cachedInputTokens, "cache_read_tokens", props.usage.coverage);
  // uncached never presents the Provider total input as non-cached input.
  // uncached 绝不把 Provider 输入总量冒充无缓存输入。
  const uncached = usageMetricDisplay(props.usage.uncachedInputTokens, "uncached_input_tokens", props.usage.coverage);
  // Output uses the shared numeric format without changing source usage.
  // 输出使用共享数值格式，不改变源用量。
  const output = usageMetricDisplay(props.usage.outputTokens, "output_tokens", props.usage.coverage);
  // Total keeps the same source precedence without extra visible qualifiers.
  // 总量保持相同来源优先级，不附加可见限定说明。
  const total = usageMetricDisplay(props.usage.totalTokens, "total_tokens", props.usage.coverage);
  return (
    <div className="vk-stats-grid">
      <TurnStatsMetricRow
        leftLabel={t("conversation.turnStats.requests")}
        leftValue={formatTurnStatsNumber(props.usage.providerCallCount, props.locale)}
        rightLabel={t("conversation.turnStats.toolCalls")}
        rightValue={formatTurnStatsNumber(props.usage.toolCallCount, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={t("conversation.turnStats.cachedTokens")}
        leftValue={cached}
        rightLabel={t("conversation.turnStats.uncachedInput")}
        rightValue={uncached}
      />
      <TurnStatsMetricRow
        leftLabel={t("conversation.turnStats.outputTokens")}
        leftValue={output}
        rightLabel={t("conversation.turnStats.totalTokens")}
        rightValue={total}
      />
      {props.children}
    </div>
  );
}

// TurnStatsMetricRow renders one aligned pair inside the statistics grid.
// TurnStatsMetricRow 在统计网格中渲染一组对齐双列。
//
// Parameters:
// 参数：左右标签和值分别描述当前行的两项统计。
// Returns one two-column metric row.
// 返回一行双列统计项。
function TurnStatsMetricRow(props: {
  // Label for the left metric.
  // 左侧统计标签。
  leftLabel: string;
  // Formatted value for the left metric.
  // 左侧统计格式化值。
  leftValue: string;
  // Label for the right metric.
  // 右侧统计标签。
  rightLabel: string;
  // Formatted value for the right metric.
  // 右侧统计格式化值。
  rightValue: string;
}) {
  return (
    <div className="vk-stats-pair">
      <div className="vk-stat">
        <span>{props.leftLabel}</span>
        <strong>{props.leftValue}</strong>
      </div>
      <div className="vk-stat">
        <span>{props.rightLabel}</span>
        <strong>{props.rightValue}</strong>
      </div>
    </div>
  );
}

// turnStatsUsageSnapshot reads one aggregate or usage-group object through the established turn_stats field contract.
// turnStatsUsageSnapshot 通过既有 turn_stats 字段契约读取一个总计或用量分组对象。
//
// Parameters:
// 参数：stats 是后端序列化的轮次统计或分组对象。
// Returns normalized non-negative counts while retaining cache-field absence.
// 返回标准化非负计数，同时保留缓存字段缺失状态。
function turnStatsUsageSnapshot(stats: Record<string, unknown>): TurnStatsUsageSnapshot {
  // cachedInputTokens keeps null when the optional Provider field was not reported.
  // cachedInputTokens 在 Provider 未报告可选字段时保留 null。
  const cachedInputTokens = optionalTurnStatsCount(stats.cached_input_tokens ?? stats.cachedInputTokens);
  // reportedUncachedInputTokens is the authoritative protocol-normalized field persisted by new turn stats.
  // reportedUncachedInputTokens 是新轮次统计持久化的权威协议归一化字段。
  const reportedUncachedInputTokens = optionalTurnStatsCount(
    stats.uncached_input_tokens ?? stats.uncachedInputTokens,
  );
  // uncachedInputTokens uses normalized segments belonging to this row when the aggregate was not persisted.
  // uncachedInputTokens 在总计未持久化时使用属于本行的归一化分段。
  const uncachedInputTokens = reportedUncachedInputTokens
    ?? segmentedTurnStatsUncachedInputTokens(stats);
  return {
    providerCallCount: numberMetadata(stats.provider_call_count ?? stats.providerCallCount),
    toolCallCount: numberMetadata(stats.tool_call_count ?? stats.toolCallCount),
    uncachedInputTokens,
    outputTokens: optionalTurnStatsCount(stats.output_tokens ?? stats.outputTokens),
    totalTokens: optionalTurnStatsCount(stats.total_tokens ?? stats.totalTokens),
    cachedInputTokens,
    coverage: readUsageCoverage(stats.usage_coverage),
  };
}

// segmentedTurnStatsUncachedInputTokens sums authoritative normalized values for this statistics owner.
// segmentedTurnStatsUncachedInputTokens 累加属于当前统计归属的权威归一化值。
//
// Parameters:
// 参数：stats 是尚未持久化 uncached_input_tokens 的旧版轮次统计对象。
// Returns the complete normalized aggregate, or null when any contributing segment is unknown.
// 返回完整的归一化总计；任一参与分段未知时返回 null。
function segmentedTurnStatsUncachedInputTokens(stats: Record<string, unknown>): number | null {
  // segmentsValue follows the persisted segment list and its existing camelCase projection.
  // segmentsValue 遵循持久化分段列表及其既有 camelCase 投影。
  const segmentsValue = stats.usage_segments ?? stats.usageSegments;
  if (!Array.isArray(segmentsValue)) {
    return null;
  }
  // turnKind uses the backend's serde default; only user turns exclude detached child segments.
  // turnKind 使用后端 serde 默认值；只有用户轮次排除分离子代理分段。
  const turnKind = metadataString(stats, ["turn_kind", "turnKind"]) ?? "user_turn";
  if (turnKind !== "user_turn" && turnKind !== "context_compaction" && turnKind !== "subagent_feedback") {
    return null;
  }
  // uncachedInputTokens follows merge_explicit_usage_segment_into_turn_stats ownership rules.
  // uncachedInputTokens 遵循 merge_explicit_usage_segment_into_turn_stats 的归属规则。
  let uncachedInputTokens = 0;
  // observed distinguishes a measured zero from the absence of contributing accounting.
  // observed 区分已测量的零值与没有参与核算的分段。
  let observed = false;
  for (const segmentValue of segmentsValue) {
    // segment validates the persisted entry before inspecting its serialized discriminator.
    // segment 在检查序列化判别字段之前验证持久化条目。
    const segment = objectValue(segmentValue);
    if (!segment) {
      return null;
    }
    // kind follows SessionTurnUsageSegmentKind; user-turn child usage belongs only to the separate group.
    // kind 遵循 SessionTurnUsageSegmentKind；用户轮次中的子代理用量仅属于独立分组。
    const kind = metadataString(segment, ["kind"]);
    if (kind === "subagent_task" && turnKind === "user_turn") {
      continue;
    }
    if (kind !== "assistant_response" && kind !== "context_compaction" && kind !== "subagent_task") {
      return null;
    }
    // normalizedInput preserves unknown usage: execution_profile is a diagnostic strategy, not a protocol.
    // normalizedInput 保留未知用量：execution_profile 是诊断策略，并非协议。
    const normalizedInput = optionalTurnStatsCount(
      segment.uncached_input_tokens ?? segment.uncachedInputTokens,
    );
    if (normalizedInput == null) {
      return null;
    }
    uncachedInputTokens += normalizedInput;
    observed = true;
  }
  return observed ? uncachedInputTokens : null;
}

// turnStatsSubagentUsageSnapshot selects the authoritative subagent usage group from a parent turn.
// turnStatsSubagentUsageSnapshot 从父轮次中选择权威子代理用量分组。
//
// Parameters:
// 参数：stats 是包含可选 usage_groups 的轮次统计对象。
// Returns the normalized subagent group, or null when no such group exists.
// 返回标准化子代理分组；不存在该分组时返回 null。
function turnStatsSubagentUsageSnapshot(stats: Record<string, unknown>): TurnStatsUsageSnapshot | null {
  // groupsValue follows the serialized snake_case contract and its existing camelCase projection.
  // groupsValue 遵循序列化 snake_case 契约及其既有 camelCase 投影。
  const groupsValue = stats.usage_groups ?? stats.usageGroups;
  if (!Array.isArray(groupsValue)) {
    return null;
  }
  for (const groupValue of groupsValue) {
    // group narrows one JSON array entry before inspecting its stable kind discriminator.
    // group 在检查稳定 kind 判别字段前收窄单个 JSON 数组项。
    const group = objectValue(groupValue);
    if (group && metadataString(group, ["kind"])?.toLowerCase() === "subagent") {
      return turnStatsUsageSnapshot(group);
    }
  }
  return null;
}

// optionalTurnStatsCount normalizes one optional Provider count without converting absence into zero.
// optionalTurnStatsCount 标准化一个可选 Provider 计数，同时不把缺失转换为零。
//
// Parameters:
// 参数：value 是未知 JSON 数值。
// Returns a non-negative safe integer, or null for missing, negative, fractional, or unsafe counts.
// 返回非负安全整数；缺失、负值、小数或不安全计数返回 null。
function optionalTurnStatsCount(value: unknown): number | null {
  // numericValue preserves missing and invalid counts instead of turning them into measurements.
  // numericValue 保留缺失与无效计数，而不是将其转成测量值。
  const numericValue = numericMetadata(value);
  return numericValue == null || !Number.isSafeInteger(numericValue) || numericValue < 0
    ? null
    : numericValue;
}

// formatTurnStatsNumber formats one exact integer using the active interface locale.
// formatTurnStatsNumber 使用当前界面语言格式化一个精确整数。
//
// Parameters:
// 参数：value 是非负统计值，locale 是当前界面语言。
// Returns a locale-grouped decimal string.
// 返回按语言分组的十进制字符串。
export function formatTurnStatsNumber(value: number, locale: LocaleCode): string {
  return value.toLocaleString(locale);
}

// turnStatsMetadataObject narrows the persisted metadata into the authoritative turn_stats object.
// turnStatsMetadataObject 将持久化元数据收窄为权威 turn_stats 对象。
//
// Parameters:
// 参数：metadata 是轮次统计块携带的未知元数据。
// Returns the structured turn_stats object, or null when unavailable.
// 返回结构化 turn_stats 对象；不可用时返回 null。
function turnStatsMetadataObject(metadata: unknown): Record<string, unknown> | null {
  return objectValue(metadataValue(metadata, ["turn_stats", "turnStats"]));
}
