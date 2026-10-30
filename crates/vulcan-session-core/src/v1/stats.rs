//! V1 token usage and turn/subagent statistics.
//! V1 token 用量与回合/子代理统计。
//!
//! Every physical Provider API call produces exactly one `api_stats` event
//! with an official usage snapshot; every truly closed turn produces exactly
//! one `turn_stats` event aggregating the per-API facts. Provider-missing
//! dimensions stay unknown (`None`) and are never faked with zero estimates.
//! 每次物理 Provider API 调用恰好产生一条携带官方用量快照的 `api_stats`
//! 事件；每个真正闭合的回合恰好产生一条聚合逐 API 事实的 `turn_stats`
//! 事件。Provider 缺失的维度保持未知(`None`)，绝不用零值估算伪装。

use serde::{Deserialize, Serialize};

/// Provider token usage normalized at extraction for one physical API call.
/// 一次物理 API 调用在提取阶段归一化的供应商 Token 用量。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageSnapshot {
    /// Complete input count, including cache reads and writes exactly once.
    /// 完整输入计数，缓存读取与写入均只包含一次。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Non-read input count, including cache writes.
    /// 未命中输入计数，包含缓存写入。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncached_input_tokens: Option<u64>,
    /// Complete output count, including independent thinking exactly once.
    /// 完整输出计数，独立思考只包含一次。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Official reasoning token count.
    /// 官方推理 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Official total token count.
    /// 官方总 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Official cache-read token count.
    /// 官方缓存读取 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    /// Official cache-write token count.
    /// 官方缓存写入 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    /// Official audio input token count.
    /// 官方音频输入 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_input_tokens: Option<u64>,
    /// Official audio output token count.
    /// 官方音频输出 token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_output_tokens: Option<u64>,
    /// Official billing tier or billing record name reported by the provider.
    /// Provider 上报的官方计费分区或计费记录名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_tier: Option<String>,
}

impl UsageSnapshot {
    /// Returns whether every usage dimension is unknown.
    /// 返回是否所有用量维度均未知。
    pub fn has_no_usage(&self) -> bool {
        self.input_tokens.is_none()
            && self.uncached_input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.reasoning_tokens.is_none()
            && self.total_tokens.is_none()
            && self.cache_read_tokens.is_none()
            && self.cache_write_tokens.is_none()
            && self.audio_input_tokens.is_none()
            && self.audio_output_tokens.is_none()
    }
}

/// Aggregated token totals over one or more usage snapshots.
/// 汇总一个或多个用量快照得到的 token 总量。
///
/// A dimension is `Some(sum)` only when every contributing snapshot reported
/// it; if any contributor left the dimension unknown, the aggregate stays
/// unknown (`None`) instead of faking a partial sum.
/// 只有每个贡献快照都上报了某维度时，该维度的聚合值才为 `Some(sum)`；
/// 只要有一个贡献者缺失该维度，聚合值就保持未知(`None`)，绝不伪造部分和。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregatedUsage {
    /// Aggregated input tokens.
    /// 聚合输入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Aggregated uncached input tokens.
    /// 聚合未缓存输入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncached_input_tokens: Option<u64>,
    /// Aggregated output tokens.
    /// 聚合输出 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Aggregated reasoning tokens.
    /// 聚合推理 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Aggregated total tokens.
    /// 聚合总 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Aggregated cache-read tokens.
    /// 聚合缓存读取 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    /// Aggregated cache-write tokens.
    /// 聚合缓存写入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    /// Aggregated audio input tokens.
    /// 聚合音频输入 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_input_tokens: Option<u64>,
    /// Aggregated audio output tokens.
    /// 聚合音频输出 token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_output_tokens: Option<u64>,
}

impl AggregatedUsage {
    /// Aggregates one sequence of usage snapshots in stream order.
    /// 按流顺序聚合一串用量快照。
    ///
    /// Each dimension becomes `Some(sum)` only when every sample reported it.
    /// 每个维度仅在全部样本都上报时才变为 `Some(sum)`。
    pub fn aggregate<'a>(samples: impl IntoIterator<Item = &'a UsageSnapshot>) -> Self {
        let mut sums = [0u64; 9];
        let mut unknown = [false; 9];
        for sample in samples {
            apply_dimension(&mut sums[0], &mut unknown[0], sample.input_tokens);
            apply_dimension(&mut sums[1], &mut unknown[1], sample.uncached_input_tokens);
            apply_dimension(&mut sums[2], &mut unknown[2], sample.output_tokens);
            apply_dimension(&mut sums[3], &mut unknown[3], sample.reasoning_tokens);
            apply_dimension(&mut sums[4], &mut unknown[4], sample.total_tokens);
            apply_dimension(&mut sums[5], &mut unknown[5], sample.cache_read_tokens);
            apply_dimension(&mut sums[6], &mut unknown[6], sample.cache_write_tokens);
            apply_dimension(&mut sums[7], &mut unknown[7], sample.audio_input_tokens);
            apply_dimension(&mut sums[8], &mut unknown[8], sample.audio_output_tokens);
        }
        Self {
            input_tokens: dimension_value(sums[0], unknown[0]),
            uncached_input_tokens: dimension_value(sums[1], unknown[1]),
            output_tokens: dimension_value(sums[2], unknown[2]),
            reasoning_tokens: dimension_value(sums[3], unknown[3]),
            total_tokens: dimension_value(sums[4], unknown[4]),
            cache_read_tokens: dimension_value(sums[5], unknown[5]),
            cache_write_tokens: dimension_value(sums[6], unknown[6]),
            audio_input_tokens: dimension_value(sums[7], unknown[7]),
            audio_output_tokens: dimension_value(sums[8], unknown[8]),
        }
    }
}

/// Marks one sample dimension as known or unknown.
/// 把一个样本维度标记为已知或未知。
fn apply_dimension(sum: &mut u64, unknown: &mut bool, value: Option<u64>) {
    match value {
        Some(added) => *sum = sum.saturating_add(added),
        None => *unknown = true,
    }
}

/// Produces the final dimension value: `None` when any contributor was unknown.
/// 生成最终维度值：只要任一贡献者未知即为 `None`。
fn dimension_value(sum: u64, unknown: bool) -> Option<u64> {
    if unknown { None } else { Some(sum) }
}

/// Additive timing evidence keeps averages and speed weighted by their measured request cohort.
/// 可累加计时证据按实测请求组对平均延迟与速度加权。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderOutputTiming {
    /// Requests with valid dispatch and completion timestamps, including nonstreaming calls.
    /// 具有有效分派及完成时间戳的请求数，包含非流式调用。
    pub total_request_count: u64,
    /// Sum of measured physical request durations, excluding tool execution.
    /// 实测物理请求总耗时，不包含工具执行。
    pub total_request_duration_ms: u64,
    /// Requests with paired first-token, output-duration and output-token observations.
    /// 首 Token、输出时长及输出 Token 均配对的请求数。
    pub request_count: u64,
    /// Sum of physical request durations for the paired output cohort.
    /// 配对输出请求组的物理请求耗时总和。
    pub request_duration_ms: u64,
    /// Sum of first-token latencies for the paired output cohort.
    /// 配对输出请求组的首 Token 延迟总和。
    pub first_token_latency_ms: u64,
    /// Sum of completion minus first-token timestamps for the paired cohort.
    /// 配对请求组的完成时间减首 Token 时间总和。
    pub output_duration_ms: u64,
    /// Output tokens from exactly the same measured output cohort.
    /// 来自同一实测输出请求组的输出 Token 数。
    pub output_tokens: u64,
}

impl ProviderOutputTiming {
    /// Extracts additive timing from one API fact; missing checkpoints remain unmeasured.
    /// 从单个 API 事实提取可累加计时；缺失计时点保持未测量。
    /// `api` owns timestamps and usage; returns evidence only when request duration is valid.
    /// `api` 持有时间戳及用量；仅在请求耗时有效时返回证据。
    pub fn from_api(api: &crate::v1::event::ApiStatsEvent) -> Option<Self> {
        // Only authoritative dispatch and completion timestamps define physical request duration.
        // 仅使用权威分派与完成时间戳定义物理请求时长。
        let (Some(start), Some(end)) = (api.timing.dispatched_at_ms, api.timing.completed_at_ms)
        else {
            return None;
        };
        if end < start {
            return None;
        }
        // Track all timed requests separately from the output-measured cohort.
        // 将全部已计时请求与已测输出请求组分别累计。
        let mut timing = Self {
            total_request_count: 1,
            total_request_duration_ms: end.abs_diff(start),
            ..Self::default()
        };
        if let (Some(first), Some(output)) = (api.timing.first_token_at_ms, api.usage.output_tokens)
            && first >= start
            && end > first
        {
            timing.request_count = 1;
            timing.request_duration_ms = end.abs_diff(start);
            timing.first_token_latency_ms = first.abs_diff(start);
            timing.output_duration_ms = end.abs_diff(first);
            timing.output_tokens = output;
        }
        Some(timing)
    }

    /// Sums independent timing buckets; no average is averaged again.
    /// 累加独立计时桶；不对平均值再次求平均。
    /// `samples` must cover disjoint requests; returns None when no sample exists.
    /// `samples` 必须覆盖互不重叠的请求；无样本时返回 None。
    pub fn aggregate<'a>(samples: impl IntoIterator<Item = &'a Self>) -> Option<Self> {
        // Raw sums preserve weighting when parent and child requests have different durations.
        // 原始总和在父子请求时长不同时保留正确权重。
        let mut result = Self::default();
        // Absence remains unknown rather than creating a zero-valued measurement.
        // 缺失保持未知，不创建零值测量。
        let mut observed = false;
        for sample in samples {
            observed = true;
            result.total_request_count += sample.total_request_count;
            result.total_request_duration_ms += sample.total_request_duration_ms;
            result.request_count += sample.request_count;
            result.request_duration_ms += sample.request_duration_ms;
            result.first_token_latency_ms += sample.first_token_latency_ms;
            result.output_duration_ms += sample.output_duration_ms;
            result.output_tokens += sample.output_tokens;
        }
        observed.then_some(result)
    }
}

/// One turn-level aggregate bucket over API, tool, and subagent facts.
/// 一个回合级聚合桶，覆盖 API、工具与子代理事实。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatBucket {
    /// Additive request timing; absent on historical buckets without measurements.
    /// 可累加请求计时；缺少测量的历史统计桶不包含此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_timing: Option<ProviderOutputTiming>,
    /// Number of physical Provider API calls in the bucket.
    /// 桶内物理 Provider API 调用数。
    pub api_call_count: u64,
    /// Number of API calls with no usage info at all.
    /// 完全没有用量信息的 API 调用数。
    pub unknown_usage_api_call_count: u64,
    /// Number of tool invocations in the bucket.
    /// 桶内工具调用数。
    pub tool_call_count: u64,
    /// Number of subagent returns counted in the bucket.
    /// 计入桶内的子代理回流次数。
    pub subagent_return_count: u64,
    /// Aggregated official token totals.
    /// 聚合的官方 token 总量。
    pub usage: AggregatedUsage,
}

impl StatBucket {
    /// Builds one bucket from its API usage samples and exact counts.
    /// 从 API 用量样本与精确计数构建一个桶。
    pub fn from_api_usages<'a>(
        api_usages: impl IntoIterator<Item = &'a UsageSnapshot>,
        api_call_count: u64,
        tool_call_count: u64,
        subagent_return_count: u64,
    ) -> Self {
        let samples: Vec<&UsageSnapshot> = api_usages.into_iter().collect();
        let unknown_usage_api_call_count =
            samples.iter().filter(|usage| usage.has_no_usage()).count() as u64;
        Self {
            api_call_count,
            unknown_usage_api_call_count,
            tool_call_count,
            subagent_return_count,
            usage: AggregatedUsage::aggregate(samples),
            output_timing: None,
        }
    }
}

/// Direct/subagent/total triple preserved by one terminal turn statistic.
/// 一条终态回合统计保留的 direct/subagent/total 三组口径。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStatBuckets {
    /// Facts owned by the parent session body itself.
    /// 父会话本体自身的事实。
    pub direct: StatBucket,
    /// Facts returned by subagents.
    /// 子代理回流的事实。
    pub subagent: StatBucket,
    /// Combined `direct + subagent` total.
    /// `direct + subagent` 的合并总量。
    pub total: StatBucket,
}

impl StatBucket {
    /// Returns whether this bucket carries no contributions at all.
    /// 返回该桶是否完全不携带任何贡献。
    pub fn is_empty(&self) -> bool {
        self.api_call_count == 0 && self.subagent_return_count == 0
    }
}

impl TurnStatBuckets {
    /// Computes the triple and verifies the total equals the sum of both sides.
    /// 计算三组口径，并校验 total 等于两侧之和。
    ///
    /// An empty side (no API/subagent contributions) is treated as identity so
    /// an absent scope never poisons the merged totals with unknown dimensions.
    /// 空侧(无 API/子代理贡献)按恒等处理，使缺失的作用域永远不会用未知
    /// 维度污染合并 total。
    pub fn combine(direct: StatBucket, subagent: StatBucket) -> Self {
        let total_usage = if direct.is_empty() {
            subagent.usage.clone()
        } else if subagent.is_empty() {
            direct.usage.clone()
        } else {
            combine_usage(&direct.usage, &subagent.usage)
        };
        let total = StatBucket {
            api_call_count: direct.api_call_count + subagent.api_call_count,
            unknown_usage_api_call_count: direct.unknown_usage_api_call_count
                + subagent.unknown_usage_api_call_count,
            tool_call_count: direct.tool_call_count + subagent.tool_call_count,
            subagent_return_count: subagent.subagent_return_count,
            usage: total_usage,
            output_timing: ProviderOutputTiming::aggregate(
                [
                    direct.output_timing.as_ref(),
                    subagent.output_timing.as_ref(),
                ]
                .into_iter()
                .flatten(),
            ),
        };
        Self {
            direct,
            subagent,
            total,
        }
    }
}

/// Merges two aggregated usages dimension by dimension.
/// 逐维度合并两份聚合用量。
fn combine_usage(left: &AggregatedUsage, right: &AggregatedUsage) -> AggregatedUsage {
    AggregatedUsage {
        input_tokens: merge_dimension(left.input_tokens, right.input_tokens),
        uncached_input_tokens: merge_dimension(
            left.uncached_input_tokens,
            right.uncached_input_tokens,
        ),
        output_tokens: merge_dimension(left.output_tokens, right.output_tokens),
        reasoning_tokens: merge_dimension(left.reasoning_tokens, right.reasoning_tokens),
        total_tokens: merge_dimension(left.total_tokens, right.total_tokens),
        cache_read_tokens: merge_dimension(left.cache_read_tokens, right.cache_read_tokens),
        cache_write_tokens: merge_dimension(left.cache_write_tokens, right.cache_write_tokens),
        audio_input_tokens: merge_dimension(left.audio_input_tokens, right.audio_input_tokens),
        audio_output_tokens: merge_dimension(left.audio_output_tokens, right.audio_output_tokens),
    }
}

/// Sums two dimension values; any unknown side keeps the merge unknown.
/// 求和两个维度值；任一侧未知则合并结果保持未知。
fn merge_dimension(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.saturating_add(b)),
        _ => None,
    }
}

/// Goal-budget token fact: `uncached_input + output` over the frozen merged scope.
/// Goal 预算 token 事实：按已冻结的归并口径计算 `uncached_input + output`。
///
/// Returns `None` when either dimension is unknown in the merged scope.
/// 归并口径内任一维度未知时返回 `None`。
pub fn goal_budget_tokens(total: &StatBucket) -> Option<u64> {
    match (total.usage.uncached_input_tokens, total.usage.output_tokens) {
        (Some(input), Some(output)) => Some(input.saturating_add(output)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::event::{
        ApiCallStatus, ApiPurposeKind, ApiStatsEvent, ApiTimings, ContinuationEligibility,
        TurnOutcome, TurnStatsEvent,
    };
    use crate::v1::ids::{ApiCallId, Seq};

    fn sample(input: u64, output: u64) -> UsageSnapshot {
        UsageSnapshot {
            input_tokens: Some(input),
            uncached_input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(input + output),
            ..UsageSnapshot::default()
        }
    }

    /// Aggregation sums every dimension across samples.
    /// 聚合对所有样本的每个维度求和。
    #[test]
    fn aggregate_sums_known_dimensions() {
        let aggregated = AggregatedUsage::aggregate([&sample(10, 20), &sample(30, 40)]);
        assert_eq!(aggregated.input_tokens, Some(40));
        assert_eq!(aggregated.output_tokens, Some(60));
        assert_eq!(aggregated.total_tokens, Some(100));
    }

    /// A single unknown contributor makes that dimension unknown, not zero.
    /// 单个未知贡献者使该维度保持未知，而不是变成零。
    #[test]
    fn aggregate_keeps_unknown_dimension_unknown() {
        let with_output = sample(10, 20);
        let without_input = UsageSnapshot {
            output_tokens: Some(5),
            ..UsageSnapshot::default()
        };
        let aggregated = AggregatedUsage::aggregate([&with_output, &without_input]);
        assert_eq!(aggregated.input_tokens, None);
        assert_eq!(aggregated.output_tokens, Some(25));
    }

    /// Unknown-usage counting only flags snapshots with no dimensions at all.
    /// 未知用量计数只标记没有任何维度的快照。
    #[test]
    fn unknown_usage_api_call_counting() {
        let empty = UsageSnapshot::default();
        let bucket = StatBucket::from_api_usages([&sample(1, 2), &empty], 2, 0, 0);
        assert_eq!(bucket.unknown_usage_api_call_count, 1);
        assert_eq!(bucket.api_call_count, 2);
    }

    /// The combined total equals direct plus subagent on every dimension.
    /// 合并 total 在每个维度上都等于 direct 加 subagent。
    #[test]
    fn combine_totals_are_consistent() {
        let direct = StatBucket::from_api_usages([&sample(10, 20)], 1, 2, 0);
        let subagent = StatBucket::from_api_usages([&sample(5, 5)], 1, 0, 1);
        let buckets = TurnStatBuckets::combine(direct, subagent);
        assert_eq!(buckets.total.api_call_count, 2);
        assert_eq!(buckets.total.tool_call_count, 2);
        assert_eq!(buckets.total.subagent_return_count, 1);
        assert_eq!(buckets.total.usage.input_tokens, Some(15));
        assert_eq!(buckets.total.usage.output_tokens, Some(25));
    }

    /// Goal budget tokens use uncached input plus output over the total scope.
    /// Goal 预算 token 在 total 口径上使用未缓存输入加输出。
    #[test]
    fn goal_budget_uses_frozen_scope() {
        let direct = StatBucket::from_api_usages([&sample(10, 20)], 1, 0, 0);
        let subagent = StatBucket::from_api_usages([&sample(5, 5)], 1, 0, 0);
        let total = TurnStatBuckets::combine(direct, subagent).total;
        assert_eq!(goal_budget_tokens(&total), Some(15 + 25));
    }

    /// Aggregation stays unknown when the scope mixes known and unknown.
    /// 归并口径混合已知与未知时聚合保持未知。
    #[test]
    fn goal_budget_unknown_when_scope_unknown() {
        let direct = StatBucket::from_api_usages([&sample(10, 20)], 1, 0, 0);
        let subagent = StatBucket::from_api_usages([&UsageSnapshot::default()], 1, 0, 0);
        let total = TurnStatBuckets::combine(direct, subagent).total;
        assert_eq!(goal_budget_tokens(&total), None);
    }

    /// ApiStats rejects unknown fields nested inside its usage snapshot.
    /// ApiStats 会拒绝 usage 快照内部的未知字段。
    #[test]
    fn api_stats_rejects_unknown_nested_usage_fields() {
        let event = ApiStatsEvent {
            api_call_id: ApiCallId::try_new(1).unwrap(),
            purpose: ApiPurposeKind::MainGeneration,
            provider: "provider".to_string(),
            profile: None,
            model: "model".to_string(),
            connection_id: None,
            thinking_level: None,
            temperature: None,
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Completed,
            end_reason: None,
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing: ApiTimings::default(),
        };
        let mut value = serde_json::to_value(event).unwrap();
        value["usage"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ApiStatsEvent>(value).is_err());
    }

    /// TurnStats rejects unknown fields nested inside any aggregate bucket.
    /// TurnStats 会拒绝任意聚合统计桶内部的未知字段。
    #[test]
    fn turn_stats_rejects_unknown_nested_bucket_fields() {
        let event = TurnStatsEvent {
            start_seq: Seq::try_new(1).unwrap(),
            end_seq: Seq::try_new(2).unwrap(),
            outcome: TurnOutcome::Completed,
            duration_ms: Some(1),
            buckets: TurnStatBuckets::default(),
        };
        let mut value = serde_json::to_value(event).unwrap();
        value["buckets"]["direct"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TurnStatsEvent>(value).is_err());
    }
}
