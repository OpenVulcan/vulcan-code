import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
  objectValue,
} from "@/components/conversation/conversation-metadata";

// TurnStatsUsageSnapshot contains only the accounting fields shown in the compact statistics card.
// TurnStatsUsageSnapshot 仅包含紧凑统计卡展示的核算字段。
interface TurnStatsUsageSnapshot {
  // Number of Provider requests represented by this scope.
  // 当前范围包含的 Provider 请求次数。
  providerCallCount: number;
  // Number of tool calls represented by this scope.
  // 当前范围包含的工具调用次数。
  toolCallCount: number;
  // Provider-reported input token count.
  // Provider 报告的输入 Token 数。
  inputTokens: number;
  // Provider-reported output token count.
  // Provider 报告的输出 Token 数。
  outputTokens: number;
  // Provider-reported total token count.
  // Provider 报告的总 Token 数。
  totalTokens: number;
  // Optional Provider-reported cache-hit input token count.
  // Provider 报告的可选缓存命中输入 Token 数。
  cachedInputTokens: number | null;
}

// TurnStatsTooltipCard renders three aligned aggregate rows, one cache-hit bar, and an optional child-agent section.
// TurnStatsTooltipCard 渲染三行对齐的总计、一条缓存命中进度条及可选子代理分区。
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
  locale: "zh-CN" | "en-US";
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
  // zh selects compact local labels without expanding the global translation contract for one private card.
  // zh 为该私有卡片选择紧凑本地标签，避免扩展全局翻译契约。
  const zh = props.locale === "zh-CN";
  // aggregate contains the complete turn totals persisted by the backend.
  // aggregate 包含后端持久化的完整轮次总计。
  const aggregate = turnStatsUsageSnapshot(stats);
  // subagent contains the first-class child-agent group only for a parent turn.
  // subagent 仅在父轮次中包含一等子代理分组。
  const subagent = props.subagentStats ? null : turnStatsSubagentUsageSnapshot(stats);
  // cacheHitPercent uses the Provider input-token total as the denominator shown in this card.
  // cacheHitPercent 使用卡片展示的 Provider 输入 Token 总数作为分母。
  const cacheHitPercent = turnStatsCacheHitPercent(aggregate);
  // speed preserves the backend-derived output speed and avoids deriving it again in the browser.
  // speed 保留后端推导的输出速度，避免浏览器再次计算。
  const speed = numericMetadata(stats.output_tokens_per_second ?? stats.outputTokensPerSecond);
  // speedLabel is a compact header value because each accounting section is intentionally limited to three rows.
  // speedLabel 作为紧凑标题值展示，使每个核算分区严格保持三行。
  const speedLabel = speed != null && Number.isFinite(speed) && speed > 0
    ? `${speed.toFixed(1)} tokens/s`
    : (zh ? "暂无速度" : "No speed data");

  return (
    <>
      <div className="context-usage-card-header turn-stats-card-header">
        <strong>{props.subagentStats ? (zh ? "子代理统计" : "Subagent usage") : (zh ? "本轮统计" : "Turn usage")}</strong>
        <span>{zh ? `平均 ${speedLabel}` : `Average ${speedLabel}`}</span>
      </div>
      <TurnStatsUsageRows locale={props.locale} usage={aggregate} />
      <div className="turn-stats-cache-summary">
        <div className="turn-stats-cache-header">
          <span>{zh ? "缓存命中率" : "Cache hit rate"}</span>
          <strong>{cacheHitPercent == null ? (zh ? "暂无数据" : "No data") : `${cacheHitPercent.toFixed(1)}%`}</strong>
        </div>
        <div
          aria-label={zh ? "缓存命中率" : "Cache hit rate"}
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={cacheHitPercent ?? undefined}
          className="turn-stats-cache-track"
          role="progressbar"
        >
          <div className="turn-stats-cache-fill" style={{ width: `${cacheHitPercent ?? 0}%` }} />
        </div>
      </div>
      {subagent ? (
        <section className="turn-stats-subagent-section">
          <div className="turn-stats-section-title">{zh ? "子代理 Tokens" : "Subagent tokens"}</div>
          <TurnStatsUsageRows locale={props.locale} usage={subagent} />
        </section>
      ) : null}
    </>
  );
}

// TurnStatsUsageRows renders exactly three aligned rows for one aggregate or child-agent scope.
// TurnStatsUsageRows 为一个总计或子代理范围严格渲染三行对齐数据。
//
// Parameters:
// 参数：usage 包含六项用量，locale 选择标签与数字格式。
// Returns a three-row statistics grid.
// 返回三行统计网格。
function TurnStatsUsageRows(props: {
  // Usage values for the current card section.
  // 当前卡片分区的用量值。
  usage: TurnStatsUsageSnapshot;
  // Active interface locale.
  // 当前界面语言。
  locale: "zh-CN" | "en-US";
}) {
  // zh selects the compact labels used in each paired row.
  // zh 选择每行双列使用的紧凑标签。
  const zh = props.locale === "zh-CN";
  // cachedValue preserves the distinction between an official zero and unavailable cache accounting.
  // cachedValue 保留官方零值与缺少缓存统计之间的区别。
  const cachedValue = props.usage.cachedInputTokens == null
    ? "—"
    : formatTurnStatsNumber(props.usage.cachedInputTokens, props.locale);
  return (
    <div className="turn-stats-metric-grid">
      <TurnStatsMetricRow
        leftLabel={zh ? "总请求次数" : "Requests"}
        leftValue={formatTurnStatsNumber(props.usage.providerCallCount, props.locale)}
        rightLabel={zh ? "工具调用次数" : "Tool calls"}
        rightValue={formatTurnStatsNumber(props.usage.toolCallCount, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "缓存 Tokens" : "Cached tokens"}
        leftValue={cachedValue}
        rightLabel={zh ? "输入 Tokens" : "Input tokens"}
        rightValue={formatTurnStatsNumber(props.usage.inputTokens, props.locale)}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "输出 Tokens" : "Output tokens"}
        leftValue={formatTurnStatsNumber(props.usage.outputTokens, props.locale)}
        rightLabel={zh ? "合计 Tokens" : "Total tokens"}
        rightValue={formatTurnStatsNumber(props.usage.totalTokens, props.locale)}
      />
    </div>
  );
}

// TurnStatsMetricRow renders one aligned pair inside the fixed three-row statistics grid.
// TurnStatsMetricRow 在固定三行统计网格中渲染一组对齐双列。
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
    <div className="turn-stats-metric-row">
      <div className="turn-stats-metric">
        <span>{props.leftLabel}</span>
        <strong>{props.leftValue}</strong>
      </div>
      <div className="turn-stats-metric">
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
  return {
    providerCallCount: numberMetadata(stats.provider_call_count ?? stats.providerCallCount),
    toolCallCount: numberMetadata(stats.tool_call_count ?? stats.toolCallCount),
    inputTokens: numberMetadata(stats.input_tokens ?? stats.inputTokens),
    outputTokens: numberMetadata(stats.output_tokens ?? stats.outputTokens),
    totalTokens: numberMetadata(stats.total_tokens ?? stats.totalTokens),
    cachedInputTokens,
  };
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
// Returns a non-negative integer or null when no finite number was supplied.
// 返回非负整数；未提供有限数值时返回 null。
function optionalTurnStatsCount(value: unknown): number | null {
  // numericValue preserves the distinction between a missing field and an explicit zero.
  // numericValue 保留缺失字段与显式零值之间的区别。
  const numericValue = numericMetadata(value);
  return numericValue == null || !Number.isFinite(numericValue)
    ? null
    : Math.max(0, Math.trunc(numericValue));
}

// turnStatsCacheHitPercent calculates the displayed cache-hit percentage from official input accounting.
// turnStatsCacheHitPercent 根据官方输入核算计算展示用缓存命中率。
//
// Parameters:
// 参数：usage 是当前卡片分区的标准化用量。
// Returns a percentage clamped to the progress-bar domain, or null when the ratio is unavailable.
// 返回限制在进度条范围内的百分比；比率不可用时返回 null。
function turnStatsCacheHitPercent(usage: TurnStatsUsageSnapshot): number | null {
  if (usage.cachedInputTokens == null || usage.inputTokens <= 0) {
    return null;
  }
  return Math.min(100, Math.max(0, (usage.cachedInputTokens / usage.inputTokens) * 100));
}

// formatTurnStatsNumber formats one exact integer using the active interface locale.
// formatTurnStatsNumber 使用当前界面语言格式化一个精确整数。
//
// Parameters:
// 参数：value 是非负统计值，locale 是当前界面语言。
// Returns a locale-grouped decimal string.
// 返回按语言分组的十进制字符串。
function formatTurnStatsNumber(value: number, locale: "zh-CN" | "en-US"): string {
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
