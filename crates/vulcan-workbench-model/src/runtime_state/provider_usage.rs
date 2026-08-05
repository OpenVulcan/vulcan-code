use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use vulcan_core::types::ProviderUsageSnapshot;

/// Maximum number of physical Provider request segments retained in one session runtime entry.
/// 单个会话运行态中保留的物理 Provider 请求分段最大数量。
pub const SESSION_PROVIDER_USAGE_SEGMENT_LIMIT: usize = 128;

/// Terminal state of one physical Provider request segment.
/// 单个物理 Provider 请求分段的终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionProviderUsageSegmentStatus {
    /// The physical request has been dispatched and has not reached a terminal event.
    /// 物理请求已分发但尚未收到终态事件。
    InFlight,
    /// The Provider returned a successful terminal response.
    /// Provider 已返回成功终态响应。
    Succeeded,
    /// The physical request ended without a successful terminal response.
    /// 物理请求未收到成功终态响应即结束。
    Failed,
}

/// Persisted usage facts for one physical Provider request.
/// 单次物理 Provider 请求的持久化用量事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderUsageSegment {
    /// Stable telemetry request identifier for this physical request.
    /// 此次物理请求的稳定遥测请求标识。
    pub provider_call_id: String,
    /// Stable execution owner category such as `main_agent` or `context_compaction`.
    /// 稳定执行归属类别，例如 `main_agent` 或 `context_compaction`。
    pub owner_kind: String,
    /// Current terminal lifecycle state of this request.
    /// 此次请求当前的终态生命周期状态。
    pub status: SessionProviderUsageSegmentStatus,
    /// Normalized usage when the Provider supplied usable usage facts.
    /// Provider 提供可用用量事实时的规范化用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderUsageSnapshot>,
    /// Provider dispatch timestamp in Unix milliseconds.
    /// Provider 分发时间的 Unix 毫秒时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// Provider terminal timestamp in Unix milliseconds.
    /// Provider 终态时间的 Unix 毫秒时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<i64>,
    /// Stable redacted error category for failed requests.
    /// 失败请求的稳定脱敏错误类别。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
    /// Stable redacted error code for failed requests.
    /// 失败请求的稳定脱敏错误代码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Live per-session Provider usage accounting synchronized at every physical request boundary.
/// 在每个物理请求边界同步的会话级 Provider 实时用量统计。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProviderUsageState {
    /// Number of physical Provider requests dispatched for this session.
    /// 此会话已分发的物理 Provider 请求数量。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub provider_call_count: usize,
    /// Number of dispatched requests that are currently awaiting a terminal event.
    /// 当前等待终态事件的已分发请求数量。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub in_flight_call_count: usize,
    /// Number of terminal requests for which no usable usage facts were supplied.
    /// 终态请求中未提供可用用量事实的数量。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub unknown_usage_call_count: usize,
    /// Most recent usable usage snapshot, independent of request success ordering.
    /// 最近一份可用用量快照，不依赖请求成功顺序。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_usage: Option<ProviderUsageSnapshot>,
    /// Cumulative usable usage across all terminal requests in this session.
    /// 此会话全部终态请求的可用用量累计值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cumulative_usage: Option<ProviderUsageSnapshot>,
    /// Monotonic revision incremented for every accepted start or terminal mutation.
    /// 每次接受开始或终态变化时递增的单调版本号。
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub usage_revision: u64,
    /// Bounded physical request segments used for diagnostics and replay.
    /// 用于诊断与重放的有界物理请求分段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<SessionProviderUsageSegment>,
    /// Durable request-id ledger that prevents duplicate accounting after diagnostic segments are trimmed.
    /// 可靠持久化的请求 ID 台账，防止诊断分段裁剪后重复记账。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub seen_provider_call_ids: BTreeSet<String>,
}

impl SessionProviderUsageState {
    /// Returns whether this accounting state has no observable Provider facts.
    /// 返回此统计状态是否尚未包含任何可观察 Provider 事实。
    pub fn is_empty(&self) -> bool {
        self.provider_call_count == 0
            && self.in_flight_call_count == 0
            && self.unknown_usage_call_count == 0
            && self.last_usage.is_none()
            && self.cumulative_usage.is_none()
            && self.usage_revision == 0
            && self.segments.is_empty()
            && self.seen_provider_call_ids.is_empty()
    }

    /// Records one physical Provider dispatch exactly once by telemetry request id.
    /// 按遥测请求标识只记录一次物理 Provider 分发。
    pub fn observe_started(
        &mut self,
        provider_call_id: impl Into<String>,
        owner_kind: impl Into<String>,
        started_at_ms: Option<i64>,
    ) -> bool {
        let provider_call_id = provider_call_id.into();
        if provider_call_id.trim().is_empty()
            || self.seen_provider_call_ids.contains(&provider_call_id)
            || self
                .segments
                .iter()
                .any(|segment| segment.provider_call_id == provider_call_id)
        {
            return false;
        }
        self.seen_provider_call_ids.insert(provider_call_id.clone());
        self.provider_call_count = self.provider_call_count.saturating_add(1);
        self.in_flight_call_count = self.in_flight_call_count.saturating_add(1);
        self.usage_revision = self.usage_revision.saturating_add(1);
        self.segments.push(SessionProviderUsageSegment {
            provider_call_id,
            owner_kind: owner_kind.into(),
            status: SessionProviderUsageSegmentStatus::InFlight,
            usage: None,
            started_at_ms,
            completed_at_ms: None,
            error_category: None,
            error_code: None,
        });
        self.trim_segments();
        true
    }

    /// Records one physical Provider terminal event exactly once by telemetry request id.
    /// 按遥测请求标识只记录一次物理 Provider 终态事件。
    // Keep terminal usage fields explicit so every provider accounting transition is auditable.
    // 保持终态用量字段显式，使每次供应商记账转换都可审计。
    #[allow(clippy::too_many_arguments)]
    pub fn observe_completed(
        &mut self,
        provider_call_id: impl Into<String>,
        owner_kind: impl Into<String>,
        status: SessionProviderUsageSegmentStatus,
        usage: Option<ProviderUsageSnapshot>,
        started_at_ms: Option<i64>,
        completed_at_ms: Option<i64>,
        error_category: Option<String>,
        error_code: Option<String>,
    ) -> bool {
        let provider_call_id = provider_call_id.into();
        if provider_call_id.trim().is_empty() {
            return false;
        }
        let segment_index = self
            .segments
            .iter()
            .position(|segment| segment.provider_call_id == provider_call_id);
        let Some(segment_index) = segment_index else {
            if self.seen_provider_call_ids.contains(&provider_call_id) {
                return false;
            }
            self.seen_provider_call_ids.insert(provider_call_id.clone());
            self.provider_call_count = self.provider_call_count.saturating_add(1);
            self.usage_revision = self.usage_revision.saturating_add(1);
            self.segments.push(SessionProviderUsageSegment {
                provider_call_id,
                owner_kind: owner_kind.into(),
                status,
                usage: usage.clone(),
                started_at_ms,
                completed_at_ms,
                error_category,
                error_code,
            });
            self.account_terminal_usage(usage);
            self.trim_segments();
            return true;
        };
        let segment = &mut self.segments[segment_index];
        if segment.status != SessionProviderUsageSegmentStatus::InFlight {
            return false;
        }
        self.seen_provider_call_ids.insert(provider_call_id);
        segment.status = status;
        segment.usage = usage.clone();
        segment.completed_at_ms = completed_at_ms;
        segment.error_category = error_category;
        segment.error_code = error_code;
        if segment.started_at_ms.is_none() {
            segment.started_at_ms = started_at_ms;
        }
        self.in_flight_call_count = self.in_flight_call_count.saturating_sub(1);
        self.usage_revision = self.usage_revision.saturating_add(1);
        self.account_terminal_usage(usage);
        true
    }

    /// Adds any usable terminal usage, including failed or cancelled requests, without fabricating unknown token components.
    /// 累加所有可用的终态用量，包括失败或取消但返回用量的请求，绝不伪造未知 Token 分项。
    fn account_terminal_usage(&mut self, usage: Option<ProviderUsageSnapshot>) {
        let Some(usage) = usage.filter(usage_has_any_value) else {
            self.unknown_usage_call_count = self.unknown_usage_call_count.saturating_add(1);
            return;
        };
        self.last_usage = Some(usage.clone());
        let cumulative = self
            .cumulative_usage
            .get_or_insert_with(ProviderUsageSnapshot::default);
        add_optional(&mut cumulative.input_tokens, usage.input_tokens);
        add_optional(
            &mut cumulative.uncached_input_tokens,
            usage.uncached_input_tokens,
        );
        add_optional(&mut cumulative.output_tokens, usage.output_tokens);
        add_optional(&mut cumulative.reasoning_tokens, usage.reasoning_tokens);
        add_optional(&mut cumulative.total_tokens, usage.total_tokens);
        add_optional(
            &mut cumulative.cached_input_tokens,
            usage.cached_input_tokens,
        );
        add_optional(
            &mut cumulative.cache_creation_input_tokens,
            usage.cache_creation_input_tokens,
        );
    }

    /// Removes the oldest terminal segments while retaining all in-flight segments.
    /// 删除最旧的终态分段，同时保留所有进行中的分段。
    fn trim_segments(&mut self) {
        while self.segments.len() > SESSION_PROVIDER_USAGE_SEGMENT_LIMIT {
            let Some(index) = self
                .segments
                .iter()
                .position(|segment| segment.status != SessionProviderUsageSegmentStatus::InFlight)
            else {
                break;
            };
            self.segments.remove(index);
        }
    }
}

/// Adds one optional token component while preserving unknown values.
/// 累加一个可选 Token 分项并保留未知值。
fn add_optional(target: &mut Option<usize>, value: Option<usize>) {
    if let Some(value) = value {
        *target = Some(target.unwrap_or_default().saturating_add(value));
    }
}

/// Returns whether a usage snapshot contains at least one known token component.
/// 返回用量快照是否至少包含一个已知 Token 分项。
fn usage_has_any_value(usage: &ProviderUsageSnapshot) -> bool {
    usage.input_tokens.is_some()
        || usage.uncached_input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.reasoning_tokens.is_some()
        || usage.total_tokens.is_some()
        || usage.cached_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
}

/// Returns whether a usize value is zero for serde omission.
/// 返回 usize 值是否为零，以供 serde 忽略字段。
fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

/// Returns whether a u64 value is zero for serde omission.
/// 返回 u64 值是否为零，以供 serde 忽略字段。
fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies start and terminal events update counts idempotently and accumulate known usage.
    /// 验证开始与终态事件按幂等方式更新计数并累计已知用量。
    #[test]
    fn physical_provider_usage_is_idempotent_and_incremental() {
        let mut state = SessionProviderUsageState::default();
        assert!(state.observe_started("call-1", "main_agent", Some(10)));
        assert!(!state.observe_started("call-1", "main_agent", Some(10)));
        assert_eq!(state.provider_call_count, 1);
        assert_eq!(state.in_flight_call_count, 1);
        let usage = ProviderUsageSnapshot {
            input_tokens: Some(7),
            uncached_input_tokens: Some(5),
            output_tokens: Some(3),
            total_tokens: Some(10),
            ..ProviderUsageSnapshot::default()
        };
        assert!(state.observe_completed(
            "call-1",
            "main_agent",
            SessionProviderUsageSegmentStatus::Succeeded,
            Some(usage),
            Some(10),
            Some(20),
            None,
            None,
        ));
        assert!(!state.observe_completed(
            "call-1",
            "main_agent",
            SessionProviderUsageSegmentStatus::Succeeded,
            None,
            Some(10),
            Some(20),
            None,
            None,
        ));
        assert_eq!(state.in_flight_call_count, 0);
        assert_eq!(state.unknown_usage_call_count, 0);
        assert_eq!(
            state
                .cumulative_usage
                .as_ref()
                .and_then(|usage| usage.total_tokens),
            Some(10)
        );
    }

    /// Verifies failed requests without usage are counted as unknown rather than fabricated as zero.
    /// 验证没有用量的失败请求会计为未知，而不会伪造成零 Token。
    #[test]
    fn failed_provider_usage_is_unknown_without_fake_tokens() {
        let mut state = SessionProviderUsageState::default();
        assert!(state.observe_started("call-1", "main_agent", None));
        assert!(state.observe_completed(
            "call-1",
            "main_agent",
            SessionProviderUsageSegmentStatus::Failed,
            None,
            None,
            None,
            Some(String::from("server_error")),
            None,
        ));
        assert_eq!(state.provider_call_count, 1);
        assert_eq!(state.unknown_usage_call_count, 1);
        assert!(state.cumulative_usage.is_none());
    }

    /// Verifies failed requests with usable Provider usage still contribute to cumulative accounting.
    /// 验证失败但带有可用 Provider 用量的请求仍会计入累计统计。
    #[test]
    fn failed_provider_usage_with_tokens_is_accumulated() {
        let mut state = SessionProviderUsageState::default();
        assert!(state.observe_started("call-1", "main_agent", None));
        assert!(state.observe_completed(
            "call-1",
            "main_agent",
            SessionProviderUsageSegmentStatus::Failed,
            Some(ProviderUsageSnapshot {
                input_tokens: Some(8),
                output_tokens: Some(2),
                total_tokens: Some(10),
                ..ProviderUsageSnapshot::default()
            }),
            None,
            None,
            Some(String::from("server_error")),
            None,
        ));
        assert_eq!(state.unknown_usage_call_count, 0);
        assert_eq!(
            state
                .cumulative_usage
                .as_ref()
                .and_then(|usage| usage.total_tokens),
            Some(10)
        );
    }

    /// Verifies request-id deduplication survives diagnostic segment trimming.
    /// 验证诊断分段裁剪后仍然按请求 ID 去重。
    #[test]
    fn trimmed_provider_usage_segment_cannot_be_counted_again() {
        let mut state = SessionProviderUsageState::default();
        for index in 0..=SESSION_PROVIDER_USAGE_SEGMENT_LIMIT {
            let provider_call_id = format!("call-{index}");
            assert!(state.observe_started(&provider_call_id, "main_agent", None));
            assert!(state.observe_completed(
                &provider_call_id,
                "main_agent",
                SessionProviderUsageSegmentStatus::Succeeded,
                Some(ProviderUsageSnapshot {
                    total_tokens: Some(1),
                    ..ProviderUsageSnapshot::default()
                }),
                None,
                None,
                None,
                None,
            ));
        }
        assert_eq!(
            state.provider_call_count,
            SESSION_PROVIDER_USAGE_SEGMENT_LIMIT + 1
        );
        assert_eq!(state.segments.len(), SESSION_PROVIDER_USAGE_SEGMENT_LIMIT);
        let duplicate_id = format!("call-{}", 0);
        assert!(!state.observe_completed(
            duplicate_id,
            "main_agent",
            SessionProviderUsageSegmentStatus::Succeeded,
            Some(ProviderUsageSnapshot {
                total_tokens: Some(99),
                ..ProviderUsageSnapshot::default()
            }),
            None,
            None,
            None,
            None,
        ));
        assert_eq!(
            state
                .cumulative_usage
                .as_ref()
                .and_then(|usage| usage.total_tokens),
            Some(SESSION_PROVIDER_USAGE_SEGMENT_LIMIT + 1)
        );
    }
}
