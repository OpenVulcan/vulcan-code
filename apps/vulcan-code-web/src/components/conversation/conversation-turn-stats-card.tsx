import {
  metadataString,
  metadataValue,
  numberMetadata,
  numericMetadata,
  objectValue,
} from "@/components/conversation/conversation-metadata";
import { readUsageCoverage, type UsageCoverage } from "@/service/conversation-usage-coverage";
import { usageCacheRatio, usageMetricDisplay } from "./conversation-usage-display";

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
  const cacheLabel = zh ? "缓存命中率" : "Cache hit rate";
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
          <span>{cacheLabel}</span>
          <strong>{cacheHitPercent == null ? (zh ? "暂无数据" : "No data") : `${cacheHitPercent.toFixed(1)}%`}</strong>
        </div>
        <div
          aria-label={cacheLabel}
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
// 参数：usage 包含当前范围的用量核算字段，locale 选择标签与数字格式。
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
  // cached displays the recorded token count as a plain number, with zero for an empty value.
  // cached 以纯数值显示已记录的 Token 数，空值显示零。
  const cached = usageMetricDisplay(props.usage.cachedInputTokens, "cache_read_tokens", props.usage.coverage, props.locale);
  // uncached never presents the Provider total input as non-cached input.
  // uncached 绝不把 Provider 输入总量冒充无缓存输入。
  const uncached = usageMetricDisplay(props.usage.uncachedInputTokens, "uncached_input_tokens", props.usage.coverage, props.locale);
  // Output uses the shared numeric format without changing source usage.
  // 输出使用共享数值格式，不改变源用量。
  const output = usageMetricDisplay(props.usage.outputTokens, "output_tokens", props.usage.coverage, props.locale);
  // Total keeps the same source precedence without extra visible qualifiers.
  // 总量保持相同来源优先级，不附加可见限定说明。
  const total = usageMetricDisplay(props.usage.totalTokens, "total_tokens", props.usage.coverage, props.locale);
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
        leftValue={cached}
        rightLabel={zh ? "无缓存输入" : "Uncached input"}
        rightValue={uncached}
      />
      <TurnStatsMetricRow
        leftLabel={zh ? "输出 Tokens" : "Output tokens"}
        leftValue={output}
        rightLabel={zh ? "合计 Tokens" : "Total tokens"}
        rightValue={total}
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
export function TurnStatsMetricRow(props: {
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
  // reportedUncachedInputTokens is the authoritative protocol-normalized field persisted by new turn stats.
  // reportedUncachedInputTokens 是新轮次统计持久化的权威协议归一化字段。
  const reportedUncachedInputTokens = optionalTurnStatsCount(
    stats.uncached_input_tokens ?? stats.uncachedInputTokens,
  );
  // uncachedInputTokens repairs legacy segmented rows only when their protocol profile proves the input semantics.
  // uncachedInputTokens 仅在协议档案能够证明输入语义时修复旧版分段行。
  const uncachedInputTokens = reportedUncachedInputTokens
    ?? legacyTurnStatsUncachedInputTokens(stats);
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

// legacyTurnStatsUncachedInputTokens reconstructs non-cached input from protocol-tagged legacy usage segments.
// legacyTurnStatsUncachedInputTokens 根据带协议标记的旧版用量分段重建无缓存输入。
//
// Parameters:
// 参数：stats 是尚未持久化 uncached_input_tokens 的旧版轮次统计对象。
// Returns a protocol-proven aggregate, or null when any contributing segment is ambiguous.
// 返回协议可证明的聚合值；任一参与分段存在歧义时返回 null。
function legacyTurnStatsUncachedInputTokens(stats: Record<string, unknown>): number | null {
  // segmentsValue reads the stable persisted list without inventing a flat-row fallback.
  // segmentsValue 读取稳定持久化分段列表，不为扁平旧行编造回退值。
  const segmentsValue = stats.usage_segments ?? stats.usageSegments;
  if (!Array.isArray(segmentsValue)) {
    return null;
  }
  // uncachedInputTokens accumulates only segments that belong to the parent aggregate.
  // uncachedInputTokens 仅聚合属于父级总计的分段。
  let uncachedInputTokens = 0;
  // observed records whether at least one accounting segment was reconstructed.
  // observed 记录是否至少重建了一个核算分段。
  let observed = false;
  for (const segmentValue of segmentsValue) {
    // segment narrows one persisted JSON entry before reading its stable fields.
    // segment 在读取稳定字段前收窄单个持久化 JSON 条目。
    const segment = objectValue(segmentValue);
    if (!segment || metadataString(segment, ["kind"])?.toLowerCase() === "subagent_task") {
      continue;
    }
    // inputTokens preserves field absence so unknown usage cannot silently become zero.
    // inputTokens 保留字段缺失状态，避免把未知用量静默转换为零。
    const inputTokens = optionalTurnStatsCount(segment.input_tokens ?? segment.inputTokens);
    if (inputTokens == null) {
      return null;
    }
    // protocolProfile identifies the exact wire usage semantics recorded on the segment.
    // protocolProfile 标识分段记录的精确线协议用量语义。
    const protocolProfile = metadataString(segment, ["execution_profile", "executionProfile"]);
    if (!protocolProfile) {
      return null;
    }
    // cachedInputTokens defaults to zero only after the protocol profile is known.
    // cachedInputTokens 仅在协议档案已知后才将缺失缓存读取视为零。
    const cachedInputTokens = optionalTurnStatsCount(
      segment.cached_input_tokens ?? segment.cachedInputTokens,
    ) ?? 0;
    // cacheCreationInputTokens is separate uncached work in the Anthropic Messages contract.
    // cacheCreationInputTokens 在 Anthropic Messages 契约中属于独立的无缓存处理量。
    const cacheCreationInputTokens = optionalTurnStatsCount(
      segment.cache_creation_input_tokens ?? segment.cacheCreationInputTokens,
    ) ?? 0;
    const segmentUncachedInputTokens = protocolProfile === "anthropic-messages.v1"
      ? inputTokens + cacheCreationInputTokens
      : Math.max(0, inputTokens - cachedInputTokens);
    uncachedInputTokens += segmentUncachedInputTokens;
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

// formatTurnStatsNumber formats one exact integer using the active interface locale.
// formatTurnStatsNumber 使用当前界面语言格式化一个精确整数。
//
// Parameters:
// 参数：value 是非负统计值，locale 是当前界面语言。
// Returns a locale-grouped decimal string.
// 返回按语言分组的十进制字符串。
export function formatTurnStatsNumber(value: number, locale: "zh-CN" | "en-US"): string {
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
