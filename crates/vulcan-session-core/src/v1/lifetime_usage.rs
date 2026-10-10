//! Aggregate-only lifetime usage shared by a root session and its descendants.
//! 主会话与后代共用的仅累计生命周期用量。

use super::{ApiTimings, UsageSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A sum and its measured sample count; absent observations never become zero samples.
/// 累计值及有效样本数；缺失观测不作为零值样本。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageMeasurement {
    /// Sum of known observations.
    /// 已知观测的累计值。
    pub sum: u64,
    /// Number of known observations, including explicit zeros.
    /// 已知观测次数，包含明确上报的零。
    pub samples: u64,
}

impl UsageMeasurement {
    /// Builds a measurement from one optional observation; returns zero samples when absent.
    /// 从一个可选观测构造累计；缺失时返回零样本。
    pub fn observe(value: Option<u64>) -> Self {
        value.map_or_else(Self::default, |sum| Self { sum, samples: 1 })
    }

    /// Adds a measured group; returns an explicit overflow error.
    /// 累加一组观测；溢出时返回明确错误。
    fn merge(&mut self, other: &Self) -> Result<(), String> {
        self.sum = add(self.sum, other.sum)?;
        self.samples = add(self.samples, other.samples)?;
        Ok(())
    }

    /// Subtracts a frozen baseline; returns none for a non-monotonic counter.
    /// 减去冻结基线；计数不单调时返回空。
    fn since(&self, baseline: &Self) -> Option<Self> {
        Some(Self {
            sum: self.sum.checked_sub(baseline.sum)?,
            samples: self.samples.checked_sub(baseline.samples)?,
        })
    }
}

/// Constant-size counters for one session's own work, or a disjoint descendant aggregate.
/// 单个会话自身工作或互不重叠后代总量的固定大小计数。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionUsageBucket {
    /// Completed physical attempts, including failed attempts.
    /// 已终态的物理尝试次数，包含失败尝试。
    pub request_count: u64,
    /// Terminal attempts without any reported usage dimension.
    /// 没有任何已上报用量维度的终态尝试次数。
    pub unknown_usage_count: u64,
    /// Newly admitted tool calls, counted once regardless of later status.
    /// 首次准入的工具次数，后续状态变化不重复计数。
    pub tool_call_count: u64,
    /// Provider-reported input tokens.
    /// Provider 上报输入 Token。
    pub input_tokens: UsageMeasurement,
    /// Normalized non-cached input tokens.
    /// 规范化非缓存输入 Token。
    pub uncached_input_tokens: UsageMeasurement,
    /// Cache-read tokens.
    /// 缓存读取 Token。
    pub cache_read_tokens: UsageMeasurement,
    /// Cache-write detail, never added again to normalized input.
    /// 缓存写入明细，不重复加到规范化输入。
    pub cache_write_tokens: UsageMeasurement,
    /// Complete output, already including any reasoning subset.
    /// 完整输出，已包含协议声明的思考子集。
    pub output_tokens: UsageMeasurement,
    /// Reasoning detail, never added again to output.
    /// 思考明细，不重复加到输出。
    pub reasoning_tokens: UsageMeasurement,
    /// Provider-reported total, independent of the Goal budget.
    /// Provider 上报总量，与 Goal 预算独立。
    pub total_tokens: UsageMeasurement,
    /// Paired uncached input plus output for Goal budget subtraction.
    /// 配对非缓存输入加输出，用于 Goal 预算差值。
    pub budget_tokens: UsageMeasurement,
    /// Dispatch-to-first-token milliseconds and valid request count.
    /// 分派到首 Token 的毫秒总量与有效请求数。
    pub first_token_ms: UsageMeasurement,
    /// Dispatch-to-terminal milliseconds and valid request count.
    /// 分派到终态的毫秒总量与有效请求数。
    pub request_duration_ms: UsageMeasurement,
    /// First-token-to-terminal milliseconds for the measured output cohort.
    /// 已测输出样本组的首 Token 到终态毫秒数。
    pub output_duration_ms: UsageMeasurement,
    /// Output tokens belonging to exactly the measured output-duration cohort.
    /// 精确属于已测输出时长样本组的输出 Token。
    pub measured_output_tokens: u64,
    /// Uncached input plus cache reads from the paired cache-rate cohort.
    /// 缓存率配对样本组的非缓存输入加缓存读取。
    pub cache_input_tokens: UsageMeasurement,
    /// Cache reads belonging to exactly the paired cache-rate cohort.
    /// 精确属于缓存率配对样本组的缓存读取。
    pub measured_cache_read_tokens: u64,
}

impl SessionUsageBucket {
    /// Builds one terminal-request increment from normalized usage and actual checkpoints.
    /// 从规范化用量及实际时间点构造一次终态请求增量。
    /// Missing facts remain unmeasured; malformed time order is excluded from timing samples.
    /// 缺失事实保持未测量；错误时间顺序不进入计时样本。
    pub fn from_api(usage: &UsageSnapshot, timing: &ApiTimings) -> Result<Self, String> {
        // Paired groups prevent unrelated missing observations from diluting averages.
        // 配对样本避免无关缺失观测稀释平均值。
        let budget = usage
            .uncached_input_tokens
            .zip(usage.output_tokens)
            .map(|(input, output)| add(input, output))
            .transpose()?;
        let cache = usage
            .uncached_input_tokens
            .zip(usage.cache_read_tokens)
            .map(|(input, read)| add(input, read).map(|total| (total, read)))
            .transpose()?;
        let output = timing
            .first_token_at_ms
            .zip(timing.completed_at_ms)
            .zip(usage.output_tokens)
            .filter(|((first, end), _)| {
                end > first && timing.dispatched_at_ms.is_some_and(|start| start <= *first)
            });
        Ok(Self {
            request_count: 1,
            unknown_usage_count: u64::from(usage.has_no_usage()),
            input_tokens: UsageMeasurement::observe(usage.input_tokens),
            uncached_input_tokens: UsageMeasurement::observe(usage.uncached_input_tokens),
            cache_read_tokens: UsageMeasurement::observe(usage.cache_read_tokens),
            cache_write_tokens: UsageMeasurement::observe(usage.cache_write_tokens),
            output_tokens: UsageMeasurement::observe(usage.output_tokens),
            reasoning_tokens: UsageMeasurement::observe(usage.reasoning_tokens),
            total_tokens: UsageMeasurement::observe(usage.total_tokens),
            budget_tokens: UsageMeasurement::observe(budget),
            first_token_ms: UsageMeasurement::observe(duration(
                timing.dispatched_at_ms,
                timing.first_token_at_ms,
            )),
            request_duration_ms: UsageMeasurement::observe(duration(
                timing.dispatched_at_ms,
                timing.completed_at_ms,
            )),
            output_duration_ms: UsageMeasurement::observe(
                output.map(|((first, end), _)| end.abs_diff(first)),
            ),
            measured_output_tokens: output.map_or(0, |(_, tokens)| tokens),
            cache_input_tokens: UsageMeasurement::observe(cache.map(|(total, _)| total)),
            measured_cache_read_tokens: cache.map_or(0, |(_, read)| read),
            ..Self::default()
        })
    }

    /// Adds a disjoint aggregate atomically; returns an overflow error without changing self.
    /// 原子累加互不重叠的累计；溢出时返回错误且不改变自身。
    pub fn merge(&mut self, increment: &Self) -> Result<(), String> {
        // Stage the fixed-size bucket before publishing any fields.
        // 先暂存固定大小的桶，再发布全部字段。
        let mut next = self.clone();
        for (value, delta) in [
            (&mut next.request_count, increment.request_count),
            (&mut next.unknown_usage_count, increment.unknown_usage_count),
            (&mut next.tool_call_count, increment.tool_call_count),
            (
                &mut next.measured_output_tokens,
                increment.measured_output_tokens,
            ),
            (
                &mut next.measured_cache_read_tokens,
                increment.measured_cache_read_tokens,
            ),
        ] {
            *value = add(*value, delta)?;
        }
        for (value, delta) in [
            (&mut next.input_tokens, &increment.input_tokens),
            (
                &mut next.uncached_input_tokens,
                &increment.uncached_input_tokens,
            ),
            (&mut next.cache_read_tokens, &increment.cache_read_tokens),
            (&mut next.cache_write_tokens, &increment.cache_write_tokens),
            (&mut next.output_tokens, &increment.output_tokens),
            (&mut next.reasoning_tokens, &increment.reasoning_tokens),
            (&mut next.total_tokens, &increment.total_tokens),
            (&mut next.budget_tokens, &increment.budget_tokens),
            (&mut next.first_token_ms, &increment.first_token_ms),
            (
                &mut next.request_duration_ms,
                &increment.request_duration_ms,
            ),
            (&mut next.output_duration_ms, &increment.output_duration_ms),
            (&mut next.cache_input_tokens, &increment.cache_input_tokens),
        ] {
            value.merge(delta)?;
        }
        *self = next;
        Ok(())
    }

    /// Returns counters since a frozen baseline, or none when the history is incompatible.
    /// 返回冻结基线之后的计数；历史不兼容时返回空。
    pub fn since(&self, baseline: &Self) -> Option<Self> {
        Some(Self {
            request_count: self.request_count.checked_sub(baseline.request_count)?,
            unknown_usage_count: self
                .unknown_usage_count
                .checked_sub(baseline.unknown_usage_count)?,
            tool_call_count: self.tool_call_count.checked_sub(baseline.tool_call_count)?,
            measured_output_tokens: self
                .measured_output_tokens
                .checked_sub(baseline.measured_output_tokens)?,
            measured_cache_read_tokens: self
                .measured_cache_read_tokens
                .checked_sub(baseline.measured_cache_read_tokens)?,
            input_tokens: self.input_tokens.since(&baseline.input_tokens)?,
            uncached_input_tokens: self
                .uncached_input_tokens
                .since(&baseline.uncached_input_tokens)?,
            cache_read_tokens: self.cache_read_tokens.since(&baseline.cache_read_tokens)?,
            cache_write_tokens: self
                .cache_write_tokens
                .since(&baseline.cache_write_tokens)?,
            output_tokens: self.output_tokens.since(&baseline.output_tokens)?,
            reasoning_tokens: self.reasoning_tokens.since(&baseline.reasoning_tokens)?,
            total_tokens: self.total_tokens.since(&baseline.total_tokens)?,
            budget_tokens: self.budget_tokens.since(&baseline.budget_tokens)?,
            first_token_ms: self.first_token_ms.since(&baseline.first_token_ms)?,
            request_duration_ms: self
                .request_duration_ms
                .since(&baseline.request_duration_ms)?,
            output_duration_ms: self
                .output_duration_ms
                .since(&baseline.output_duration_ms)?,
            cache_input_tokens: self
                .cache_input_tokens
                .since(&baseline.cache_input_tokens)?,
        })
    }

    /// Returns a known budget only when every request reported both budget dimensions.
    /// 仅当每次请求均上报两个预算维度时返回已知预算。
    pub fn known_budget(&self) -> Option<u64> {
        (self.budget_tokens.samples == self.request_count).then_some(self.budget_tokens.sum)
    }
}

/// Frozen two-scope baseline also used as the current Goal decision input.
/// 同时用于 Goal 当前决策输入的双范围冻结基线。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GoalUsageSnapshot {
    /// Main session's own counters.
    /// 主会话自身计数。
    pub main: SessionUsageBucket,
    /// Disjoint descendant counters.
    /// 互不重叠的后代计数。
    pub children: SessionUsageBucket,
}

impl GoalUsageSnapshot {
    /// Subtracts a baseline and returns a combined internal Goal bucket, never a UI lifetime total.
    /// 减去基线返回 Goal 内部合并桶，不用于界面生命周期总计。
    pub fn since(&self, baseline: &Self) -> Option<SessionUsageBucket> {
        // Budget subtraction uses measured counts too, so pre-Goal unknowns cancel out.
        // 预算同时对样本计数做差，抵消 Goal 之前的未知请求。
        let mut delta = self.main.since(&baseline.main)?;
        delta
            .merge(&self.children.since(&baseline.children)?)
            .ok()?;
        Some(delta)
    }
}

/// On-disk schema version of the root-owned usage.json document.
/// 根会话 usage.json 文档的磁盘结构版本。
pub const SESSION_LIFETIME_USAGE_VERSION: u32 = 2;

/// One stable child session and its own lifetime counters.
/// 一个稳定子会话及其自身生命周期计数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChildSessionUsage {
    /// Verified direct parent session.
    /// 经验证的直接父会话。
    pub parent_session_id: String,
    /// Only this child's own work.
    /// 仅该子会话自身工作。
    pub usage: SessionUsageBucket,
}

/// The sole root-owned usage.json document; size grows only with child count.
/// 根会话唯一的 usage.json 文档；大小只随子会话数量增长。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionLifetimeUsage {
    /// Supported on-disk schema version.
    /// 受支持的磁盘结构版本。
    pub version: u32,
    /// Stable root session identity.
    /// 稳定根会话身份。
    pub root_session_id: String,
    /// Monotonic publication revision.
    /// 单调发布版本。
    pub revision: u64,
    /// Last aggregate change time in milliseconds.
    /// 最后累计变更的毫秒时间。
    pub updated_at_ms: i64,
    /// Own root counters.
    /// 根会话自身计数。
    pub main: SessionUsageBucket,
    /// Precomputed sum of every child's own counters.
    /// 每个子会话自身计数的预计算总和。
    pub children_total: SessionUsageBucket,
    /// Stable child IDs, including completed and closed children.
    /// 稳定子会话 ID，包含已完成和已关闭子会话。
    pub children_by_session_id: BTreeMap<String, ChildSessionUsage>,
}

impl SessionLifetimeUsage {
    /// Initializes a newly created root; no historical requests are imported.
    /// 初始化新建根会话；不导入历史请求。
    pub fn new(root_session_id: String, now_ms: i64) -> Self {
        Self {
            version: SESSION_LIFETIME_USAGE_VERSION,
            root_session_id,
            revision: 0,
            updated_at_ms: now_ms,
            main: SessionUsageBucket::default(),
            children_total: SessionUsageBucket::default(),
            children_by_session_id: BTreeMap::new(),
        }
    }

    /// Captures both fixed-size scopes for Goal creation or budget decisions.
    /// 为 Goal 创建或预算决策捕获两个固定大小范围。
    pub fn goal_snapshot(&self) -> GoalUsageSnapshot {
        GoalUsageSnapshot {
            main: self.main.clone(),
            children: self.children_total.clone(),
        }
    }

    /// Applies an increment to the verified owner; unknown child identities are explicit errors.
    /// 向经验证的归属方应用增量；未知子会话身份返回明确错误。
    pub fn apply(
        &mut self,
        session_id: &str,
        increment: &SessionUsageBucket,
        now_ms: i64,
    ) -> Result<(), String> {
        // Stage the document so failed arithmetic cannot publish only one of the child scopes.
        // 暂存文档，避免算术失败仅发布一个子会话范围。
        let mut next = self.clone();
        if session_id == next.root_session_id {
            next.main.merge(increment)?;
        } else {
            next.children_by_session_id
                .get_mut(session_id)
                .ok_or_else(|| format!("Unregistered usage child: {session_id}"))?
                .usage
                .merge(increment)?;
            next.children_total.merge(increment)?;
        }
        next.revision = add(next.revision, 1)?;
        next.updated_at_ms = next.updated_at_ms.max(now_ms);
        *self = next;
        Ok(())
    }
}

/// Computes an ordered optional duration without inventing missing checkpoints.
/// 计算有序可选时间差，不虚构缺失时间点。
fn duration(start: Option<i64>, end: Option<i64>) -> Option<u64> {
    start
        .zip(end)
        .filter(|(start, end)| end >= start)
        .map(|(start, end)| end.abs_diff(start))
}

/// Adds monotonic counters; returns a visible error instead of wrapping or saturating.
/// 累加单调计数；溢出时明确报错，不回绕或饱和。
fn add(left: u64, right: u64) -> Result<u64, String> {
    left.checked_add(right)
        .ok_or_else(|| "Session usage counter overflow".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a fully reported request with a paired cache cohort.
    /// 构造完整上报且具备配对缓存样本的请求。
    fn reported() -> UsageSnapshot {
        UsageSnapshot {
            input_tokens: Some(100),
            uncached_input_tokens: Some(40),
            cache_read_tokens: Some(60),
            output_tokens: Some(20),
            total_tokens: Some(120),
            ..Default::default()
        }
    }

    /// Unknown prior history cancels at the Goal baseline; new unknown history does not.
    /// Goal 基线抵消此前未知历史，但不抵消新增未知历史。
    #[test]
    fn goal_difference_includes_live_children_and_preserves_unknowns() {
        let mut current = GoalUsageSnapshot {
            main: SessionUsageBucket::from_api(&UsageSnapshot::default(), &ApiTimings::default())
                .unwrap(),
            ..GoalUsageSnapshot::default()
        };
        let baseline = current.clone();
        current
            .main
            .merge(&SessionUsageBucket::from_api(&reported(), &ApiTimings::default()).unwrap())
            .unwrap();
        current
            .children
            .merge(&SessionUsageBucket::from_api(&reported(), &ApiTimings::default()).unwrap())
            .unwrap();
        let delta = current.since(&baseline).unwrap();
        assert_eq!(delta.request_count, 2);
        assert_eq!(delta.known_budget(), Some(120));
        current
            .children
            .merge(
                &SessionUsageBucket::from_api(&UsageSnapshot::default(), &ApiTimings::default())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(current.since(&baseline).unwrap().known_budget(), None);
    }

    /// Missing and invalid timing never dilute speed; a measured zero latency is valid.
    /// 缺失及无效计时不会稀释速度；实测零延迟有效。
    #[test]
    fn paired_measurements_exclude_unrelated_missing_samples() {
        let timing = ApiTimings {
            dispatched_at_ms: Some(10),
            first_token_at_ms: Some(10),
            completed_at_ms: Some(30),
            ..Default::default()
        };
        let mut total = SessionUsageBucket::from_api(&reported(), &timing).unwrap();
        total
            .merge(&SessionUsageBucket::from_api(&reported(), &ApiTimings::default()).unwrap())
            .unwrap();
        assert_eq!(
            total.first_token_ms,
            UsageMeasurement { sum: 0, samples: 1 }
        );
        assert_eq!(
            total.output_duration_ms,
            UsageMeasurement {
                sum: 20,
                samples: 1
            }
        );
        assert_eq!(total.measured_output_tokens, 20);
        assert_eq!(total.cache_input_tokens.sum, 200);
        assert_eq!(total.measured_cache_read_tokens, 120);
        let invalid = ApiTimings {
            dispatched_at_ms: Some(11),
            ..timing
        };
        assert_eq!(
            SessionUsageBucket::from_api(&reported(), &invalid)
                .unwrap()
                .output_duration_ms
                .samples,
            0
        );
    }

    /// Partial independent fields must not be combined into a fictitious paired sample.
    /// 独立缺失的字段不得拼成虚构的配对样本。
    #[test]
    fn cache_and_budget_require_same_request_components() {
        let mut total = SessionUsageBucket::from_api(
            &UsageSnapshot {
                uncached_input_tokens: Some(40),
                ..Default::default()
            },
            &ApiTimings::default(),
        )
        .unwrap();
        total
            .merge(
                &SessionUsageBucket::from_api(
                    &UsageSnapshot {
                        cache_read_tokens: Some(60),
                        output_tokens: Some(20),
                        ..Default::default()
                    },
                    &ApiTimings::default(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(total.cache_input_tokens.samples, 0);
        assert_eq!(total.budget_tokens.samples, 0);
        assert_eq!(total.known_budget(), None);
    }

    /// Root, child and nested child scopes remain disjoint through repeated requests.
    /// 多次请求中根、子会话与嵌套子会话范围保持互不重叠。
    #[test]
    fn nested_agents_do_not_inflate_their_parent_bucket() {
        let mut ledger = SessionLifetimeUsage::new("root".into(), 0);
        for (id, parent) in [("a", "root"), ("b", "a")] {
            ledger.children_by_session_id.insert(
                id.into(),
                ChildSessionUsage {
                    parent_session_id: parent.into(),
                    usage: Default::default(),
                },
            );
        }
        let increment = SessionUsageBucket::from_api(&reported(), &ApiTimings::default()).unwrap();
        for id in ["root", "a", "b", "b"] {
            ledger.apply(id, &increment, 1).unwrap();
        }
        assert_eq!(ledger.main.request_count, 1);
        assert_eq!(ledger.children_by_session_id["a"].usage.request_count, 1);
        assert_eq!(ledger.children_by_session_id["b"].usage.request_count, 2);
        assert_eq!(ledger.children_total.request_count, 3);
        assert_eq!(ledger.children_by_session_id.len(), 2);
    }

    /// Counter overflow leaves the complete published snapshot unchanged.
    /// 计数溢出时保持完整已发布快照不变。
    #[test]
    fn overflow_is_failure_atomic() {
        let mut ledger = SessionLifetimeUsage::new("root".into(), 0);
        ledger.main.request_count = u64::MAX;
        let before = ledger.clone();
        let increment = SessionUsageBucket::from_api(&reported(), &ApiTimings::default()).unwrap();
        assert!(ledger.apply("root", &increment, 1).is_err());
        assert_eq!(ledger, before);
    }
}
