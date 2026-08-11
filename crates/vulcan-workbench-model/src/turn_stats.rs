//! Per-turn transcript statistics shared by projections and persistence.
//! 供投影与持久化复用的按轮次转录统计模型。

use serde::{Deserialize, Serialize};

/// Logical owner kind for one completed turn statistics snapshot.
/// 单条已完成轮次统计快照的逻辑归属类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnStatsKind {
    /// Normal user-authored conversation turn.
    /// 普通用户发起的会话轮次。
    UserTurn,
    /// Host-owned context compaction turn.
    /// 宿主拥有的上下文压缩轮次。
    ContextCompaction,
    /// Display-only subagent feedback accounting row.
    /// 仅用于显示的智能体反馈统计行。
    SubagentFeedback,
}

impl Default for SessionTurnStatsKind {
    /// Returns the legacy-compatible default turn kind.
    /// 返回兼容旧数据的默认轮次类型。
    fn default() -> Self {
        Self::UserTurn
    }
}

/// Usage segment kind recorded inside an aggregated turn statistics snapshot.
/// 聚合轮次统计快照内部记录的用量分段类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnUsageSegmentKind {
    /// Main assistant response generated for the user turn.
    /// 为用户轮次生成的主助手响应。
    AssistantResponse,
    /// Detached subagent task usage embedded into the owning parent turn.
    /// 嵌入所属父会话回合的分离子代理任务用量。
    SubagentTask,
    /// Context-compaction request executed before or as the current turn.
    /// 当前轮次之前或内部执行的上下文压缩请求。
    ContextCompaction,
}

/// Aggregated owner group for one or more usage segments.
/// 一个或多个用量分段归并后的聚合归属分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnUsageGroupKind {
    /// Normal assistant work for the current conversation.
    /// 当前会话中的常规助手工作。
    Regular,
    /// Detached child-agent work reported back to the parent conversation.
    /// 回流到父会话的分离智能体工作。
    Subagent,
    /// Context compaction work associated with the turn.
    /// 与当前轮次关联的上下文压缩工作。
    ContextCompaction,
}

impl SessionTurnUsageGroupKind {
    /// Maps one detailed usage segment kind to its stable aggregate group.
    /// 将单个明细用量分段类型映射到稳定聚合分组。
    pub const fn from_segment_kind(kind: SessionTurnUsageSegmentKind) -> Self {
        match kind {
            SessionTurnUsageSegmentKind::AssistantResponse => Self::Regular,
            SessionTurnUsageSegmentKind::SubagentTask => Self::Subagent,
            SessionTurnUsageSegmentKind::ContextCompaction => Self::ContextCompaction,
        }
    }

    /// Returns the human-readable label shown in transcript details.
    /// 返回转录详情中展示的人类可读标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Regular => "常规",
            Self::Subagent => "智能体",
            Self::ContextCompaction => "压缩",
        }
    }
}

/// Provider usage segment retained for per-turn accounting breakdowns.
/// 为按轮次用量明细保留的 Provider 用量分段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnUsageSegment {
    /// Stable segment kind used by UI and accounting surfaces.
    /// 供 UI 与计费统计表面使用的稳定分段类型。
    pub kind: SessionTurnUsageSegmentKind,
    /// Optional trigger label such as manual or auto compaction.
    /// 可选触发来源标签，例如手动或自动压缩。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// Optional execution profile label for diagnostic breakdowns.
    /// 用于诊断明细的可选执行配置标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_profile: Option<String>,
    /// Optional provider identifier associated with the segment.
    /// 与该分段关联的可选 Provider 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Provider display name frozen when the represented call occurred.
    /// 所代表调用发生时冻结的 Provider 显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_name_snapshot: Option<String>,
    /// Optional model identifier associated with the segment.
    /// 与该分段关联的可选模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Model display name frozen when the represented call occurred.
    /// 所代表调用发生时冻结的模型显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name_snapshot: Option<String>,
    /// Provider call count represented by this segment.
    /// 该分段代表的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
    /// Tool call count represented by this segment.
    /// 该分段代表的工具调用次数。
    #[serde(default)]
    pub tool_call_count: usize,
    /// Provider-reported input token count.
    /// Provider 报告的输入 Token 数。
    #[serde(default)]
    pub input_tokens: usize,
    /// Provider-reported output token count.
    /// Provider 报告的输出 Token 数。
    #[serde(default)]
    pub output_tokens: usize,
    /// Provider-reported reasoning token count.
    /// Provider 报告的推理 Token 数。
    #[serde(default)]
    pub reasoning_tokens: usize,
    /// Provider-reported total token count.
    /// Provider 报告的总 Token 数。
    #[serde(default)]
    pub total_tokens: usize,
    /// Optional provider-reported cache-hit input token count.
    /// Provider 报告的可选缓存命中输入 Token 数。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider-reported cache-creation input token count.
    /// Provider 报告的可选缓存写入输入 Token 数。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
}

impl SessionTurnUsageSegment {
    /// Builds one usage segment from an optional provider usage snapshot.
    /// 根据可选 Provider 用量快照构建一个用量分段。
    pub fn from_provider_usage(
        kind: SessionTurnUsageSegmentKind,
        provider_call_count: usize,
        usage: Option<&vulcan_core::types::ProviderUsageSnapshot>,
    ) -> Self {
        let input_tokens = usage.and_then(|usage| usage.input_tokens).unwrap_or(0);
        let output_tokens = usage.and_then(|usage| usage.output_tokens).unwrap_or(0);
        let reasoning_tokens = usage.and_then(|usage| usage.reasoning_tokens).unwrap_or(0);
        let total_tokens = usage
            .and_then(|usage| usage.total_tokens)
            .unwrap_or_else(|| {
                input_tokens
                    .saturating_add(output_tokens)
                    .saturating_add(reasoning_tokens)
            });
        Self {
            kind,
            trigger: None,
            execution_profile: None,
            provider_id: None,
            provider_name_snapshot: None,
            model_id: None,
            model_name_snapshot: None,
            provider_call_count,
            tool_call_count: 0,
            input_tokens,
            output_tokens,
            reasoning_tokens,
            total_tokens,
            cached_input_tokens: usage.and_then(|usage| usage.cached_input_tokens),
            cache_creation_input_tokens: usage.and_then(|usage| usage.cache_creation_input_tokens),
        }
    }
}

/// Aggregated usage group retained for first-class regular versus child-agent display.
/// 为一等常规与智能体展示保留的聚合用量分组。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnUsageGroup {
    /// Stable group kind used by UI and accounting surfaces.
    /// 供 UI 与计费统计表面使用的稳定分组类型。
    pub kind: SessionTurnUsageGroupKind,
    /// Provider call count represented by this group.
    /// 该分组代表的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
    /// Tool call count represented by this group.
    /// 该分组代表的工具调用次数。
    #[serde(default)]
    pub tool_call_count: usize,
    /// Provider-reported input token count for this group.
    /// 该分组内 Provider 报告的输入 Token 数。
    #[serde(default)]
    pub input_tokens: usize,
    /// Provider-reported output token count for this group.
    /// 该分组内 Provider 报告的输出 Token 数。
    #[serde(default)]
    pub output_tokens: usize,
    /// Provider-reported reasoning token count for this group.
    /// 该分组内 Provider 报告的推理 Token 数。
    #[serde(default)]
    pub reasoning_tokens: usize,
    /// Provider-reported total token count for this group.
    /// 该分组内 Provider 报告的总 Token 数。
    #[serde(default)]
    pub total_tokens: usize,
    /// Optional provider-reported cache-hit input token count for this group.
    /// 该分组内 Provider 报告的可选缓存命中输入 Token 数。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider-reported cache-creation input token count for this group.
    /// 该分组内 Provider 报告的可选缓存写入输入 Token 数。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
}

impl SessionTurnUsageGroup {
    /// Builds an empty aggregate group for one group kind.
    /// 为单个分组类型构建空聚合分组。
    pub fn empty(kind: SessionTurnUsageGroupKind) -> Self {
        Self {
            kind,
            provider_call_count: 0,
            tool_call_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            total_tokens: 0,
            cached_input_tokens: None,
            cache_creation_input_tokens: None,
        }
    }

    /// Merges one usage segment into this group.
    /// 将单个用量分段合并到当前分组。
    pub fn merge_segment(&mut self, segment: &SessionTurnUsageSegment) {
        self.provider_call_count = self
            .provider_call_count
            .saturating_add(segment.provider_call_count);
        self.tool_call_count = self.tool_call_count.saturating_add(segment.tool_call_count);
        self.input_tokens = self.input_tokens.saturating_add(segment.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(segment.output_tokens);
        self.reasoning_tokens = self
            .reasoning_tokens
            .saturating_add(segment.reasoning_tokens);
        self.total_tokens = self.total_tokens.saturating_add(segment.total_tokens);
        if let Some(cached_input_tokens) = segment.cached_input_tokens {
            let accumulated = self.cached_input_tokens.unwrap_or(0);
            self.cached_input_tokens = Some(accumulated.saturating_add(cached_input_tokens));
        }
        if let Some(cache_creation_input_tokens) = segment.cache_creation_input_tokens {
            let accumulated = self.cache_creation_input_tokens.unwrap_or(0);
            self.cache_creation_input_tokens =
                Some(accumulated.saturating_add(cache_creation_input_tokens));
        }
    }

    /// Returns whether the group contains any visible accounting data.
    /// 返回该分组是否包含任何可展示统计数据。
    pub fn has_accounting(&self) -> bool {
        self.provider_call_count > 0
            || self.tool_call_count > 0
            || self.input_tokens > 0
            || self.output_tokens > 0
            || self.reasoning_tokens > 0
            || self.total_tokens > 0
            || self.cached_input_tokens.is_some()
            || self.cache_creation_input_tokens.is_some()
    }
}

/// Terminal status recorded for one completed or aborted turn statistics snapshot.
/// 为单次已完成或已中止轮次统计快照记录的终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnStatsStatus {
    /// Turn completed successfully and produced a summary.
    /// 该轮次成功完成并产出了摘要。
    Completed,
    /// Turn terminated because the run failed.
    /// 该轮次因执行失败而终止。
    Failed,
    /// Turn terminated because the run was stopped manually or interrupted.
    /// 该轮次因手动停止或主动中断而终止。
    Stopped,
}

impl SessionTurnStatsStatus {
    /// Returns the compact persisted label exposed to transcript metadata and chips.
    /// 返回暴露给转录元数据与统计标签的紧凑持久化文本。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    /// Returns the human-readable label shown in transcript summaries and hover details.
    /// 返回展示在转录摘要与悬浮详情中的人类可读标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Completed => "已完成",
            Self::Failed => "失败",
            Self::Stopped => "已中止",
        }
    }
}

impl Default for SessionTurnStatsStatus {
    /// Returns the default turn status used by legacy deserialization fallbacks.
    /// 返回旧版本反序列化回退时使用的默认轮次状态。
    fn default() -> Self {
        Self::Completed
    }
}

/// Aggregated per-turn statistics derived after one whole turn reaches a terminal state.
/// 在单个完整轮次到达终态后派生出的按轮次聚合统计快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnStats {
    /// Stable status describing how the turn ended.
    /// 描述该轮次如何结束的稳定状态。
    pub status: SessionTurnStatsStatus,
    /// Logical kind describing the owner of this statistics row.
    /// 描述该统计行归属方的逻辑类型。
    #[serde(default)]
    pub turn_kind: SessionTurnStatsKind,
    /// Provider usage breakdowns that compose the aggregate fields.
    /// 组成聚合字段的 Provider 用量分段明细。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_segments: Vec<SessionTurnUsageSegment>,
    /// First-class usage groups derived from the detailed usage segments.
    /// 从明细用量分段派生出的一等用量分组。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usage_groups: Vec<SessionTurnUsageGroup>,
    /// Number of provider calls aggregated inside the whole turn.
    /// 在整个轮次内聚合得到的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
    /// Provider calls whose response contained no usable Provider Usage.
    /// 响应中没有可用 Provider Usage 的 Provider 调用数量。
    #[serde(default)]
    pub unknown_usage_call_count: usize,
    /// Provider-call ledger revision used to materialize this view.
    /// 物化当前视图时使用的 Provider 调用账本修订号。
    #[serde(default)]
    pub source_ledger_revision: u64,
    /// Highest child-contribution revision used to materialize this view.
    /// 物化当前视图时使用的最高子代理贡献修订号。
    #[serde(default)]
    pub child_contribution_revision: u64,
    /// Whether budget-only estimates were observed but excluded from Provider Usage totals.
    /// 是否观测到已排除在 Provider Usage 汇总之外的预算估算。
    #[serde(default)]
    pub contains_estimated_budget: bool,
    /// Number of tool-call records aggregated inside the whole turn.
    /// 在整个轮次内聚合得到的工具调用记录数量。
    #[serde(default)]
    pub tool_call_count: usize,
    /// Provider-reported total input tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告输入 Token 总数。
    #[serde(default)]
    pub input_tokens: usize,
    /// Provider-reported total output tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告输出 Token 总数。
    #[serde(default)]
    pub output_tokens: usize,
    /// Provider-reported total reasoning tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告推理 Token 总数。
    #[serde(default)]
    pub reasoning_tokens: usize,
    /// Provider-reported total tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告总 Token 数。
    #[serde(default)]
    pub total_tokens: usize,
    /// Optional provider cache-hit token count when one official field is available.
    /// 当官方字段可用时记录的可选 Provider 缓存命中 Token 数。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider cache-write token count when one official field is available.
    /// 当官方字段可用时记录的可选 Provider 缓存写入 Token 数。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
    /// Best-effort duration of the whole turn in milliseconds.
    /// 整个轮次的最佳努力持续时长，单位毫秒。
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Best-effort output-token speed derived from official output tokens and turn duration.
    /// 根据官方输出 Token 与轮次耗时推导出的最佳努力输出速度。
    #[serde(default)]
    pub output_tokens_per_second: Option<f64>,
}

impl SessionTurnStats {
    /// Merges one usage segment into aggregate turn statistics and first-class usage groups.
    /// 将单个用量分段合并到聚合轮次统计与一等用量分组。
    ///
    /// # Parameters
    /// 参数
    /// `segment` is one provider, tool, compaction, or child-agent usage segment.
    /// `segment` 是单个 Provider、工具、压缩或智能体用量分段。
    pub fn merge_usage_segment(&mut self, segment: &SessionTurnUsageSegment) {
        self.provider_call_count = self
            .provider_call_count
            .saturating_add(segment.provider_call_count);
        self.tool_call_count = self.tool_call_count.saturating_add(segment.tool_call_count);
        self.input_tokens = self.input_tokens.saturating_add(segment.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(segment.output_tokens);
        self.reasoning_tokens = self
            .reasoning_tokens
            .saturating_add(segment.reasoning_tokens);
        self.total_tokens = self.total_tokens.saturating_add(segment.total_tokens);
        if let Some(cached_input_tokens) = segment.cached_input_tokens {
            let accumulated = self.cached_input_tokens.unwrap_or(0);
            self.cached_input_tokens = Some(accumulated.saturating_add(cached_input_tokens));
        }
        if let Some(cache_creation_input_tokens) = segment.cache_creation_input_tokens {
            let accumulated = self.cache_creation_input_tokens.unwrap_or(0);
            self.cache_creation_input_tokens =
                Some(accumulated.saturating_add(cache_creation_input_tokens));
        }
        self.merge_usage_segment_group(segment);
    }

    /// Merges one usage segment into the first-class usage groups only.
    /// 只将单个用量分段合并到一等用量分组。
    pub fn merge_usage_segment_group(&mut self, segment: &SessionTurnUsageSegment) {
        let group_kind = SessionTurnUsageGroupKind::from_segment_kind(segment.kind);
        let group_index = self
            .usage_groups
            .iter()
            .position(|group| group.kind == group_kind);
        match group_index {
            Some(index) => self.usage_groups[index].merge_segment(segment),
            None => {
                let mut group = SessionTurnUsageGroup::empty(group_kind);
                group.merge_segment(segment);
                self.usage_groups.push(group);
            }
        }
        self.usage_groups
            .retain(SessionTurnUsageGroup::has_accounting);
    }

    /// Returns a compact one-line summary used by transcript chips and lightweight projections.
    /// 返回供转录统计标签与轻量投影使用的紧凑单行摘要。
    pub fn summary_line(&self) -> String {
        let mut parts = vec![
            format!("{} 次调用", self.provider_call_count),
            format!("{} 个工具", self.tool_call_count),
        ];
        if self.total_tokens > 0 {
            parts.push(format!("{} tokens", self.total_tokens));
        }
        if let Some(cached_input_tokens) = self.cached_input_tokens
            && cached_input_tokens > 0
        {
            parts.push(format!("缓存命中 {}", cached_input_tokens));
        }
        if let Some(speed) = self.output_tokens_per_second
            && speed.is_finite()
            && speed > 0.0
        {
            parts.push(format!("{speed:.1}/s"));
        }
        if let Some(subagent_group) = self
            .usage_groups
            .iter()
            .find(|group| group.kind == SessionTurnUsageGroupKind::Subagent)
            && subagent_group.total_tokens > 0
        {
            parts.push(format!("智能体 {}", subagent_group.total_tokens));
        }
        parts.join(" · ")
    }

    /// Returns a multi-line human-readable detail body used by raw-source and future hover/detail surfaces.
    /// 返回供原文与后续悬浮或详情表面使用的多行人类可读详情正文。
    pub fn detail_body(&self) -> String {
        let mut lines = vec![
            format!("状态：{}", self.status.display_label()),
            format!("Provider 调用：{}", self.provider_call_count),
            format!("工具调用：{}", self.tool_call_count),
            format!("输入 Tokens：{}", self.input_tokens),
            format!("输出 Tokens：{}", self.output_tokens),
            format!("思考 Tokens：{}", self.reasoning_tokens),
            format!("总 Tokens：{}", self.total_tokens),
        ];
        if let Some(duration_ms) = self.duration_ms {
            lines.push(format!("耗时：{duration_ms} ms"));
        }
        if let Some(speed) = self.output_tokens_per_second
            && speed.is_finite()
        {
            lines.push(format!("输出速度：{speed:.3} tokens/s"));
        }
        if let Some(cached_input_tokens) = self.cached_input_tokens {
            lines.push(format!("缓存命中 Tokens：{cached_input_tokens}"));
        }
        if let Some(cache_creation_input_tokens) = self.cache_creation_input_tokens {
            lines.push(format!("缓存写入 Tokens：{cache_creation_input_tokens}"));
        }
        if !self.usage_groups.is_empty() {
            lines.push(String::from("用量分组："));
            for group in &self.usage_groups {
                lines.push(format!(
                    "{}：Provider {}，工具 {}，输入 {}，输出 {}，思考 {}，总 {}",
                    group.kind.display_label(),
                    group.provider_call_count,
                    group.tool_call_count,
                    group.input_tokens,
                    group.output_tokens,
                    group.reasoning_tokens,
                    group.total_tokens
                ));
                if let Some(cached_input_tokens) = group.cached_input_tokens {
                    lines.push(format!(
                        "{}缓存命中 Tokens：{}",
                        group.kind.display_label(),
                        cached_input_tokens
                    ));
                }
                if let Some(cache_creation_input_tokens) = group.cache_creation_input_tokens {
                    lines.push(format!(
                        "{}缓存写入 Tokens：{}",
                        group.kind.display_label(),
                        cache_creation_input_tokens
                    ));
                }
            }
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Verifies legacy serialized turn stats without segmented fields remain readable.
    /// 验证缺少分段字段的旧版序列化轮次统计仍可读取。
    #[test]
    fn session_turn_stats_deserialize_legacy_payload_with_default_segments() {
        let payload = json!({
            "status": "completed",
            "provider_call_count": 1,
            "tool_call_count": 0,
            "input_tokens": 10,
            "output_tokens": 4,
            "total_tokens": 14
        });

        let stats: SessionTurnStats =
            serde_json::from_value(payload).expect("legacy stats should deserialize");

        assert_eq!(stats.turn_kind, SessionTurnStatsKind::UserTurn);
        assert!(stats.usage_segments.is_empty());
        assert!(stats.usage_groups.is_empty());
        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.total_tokens, 14);
    }

    /// Verifies usage segments are grouped as regular and child-agent accounting.
    /// 验证用量分段会被分组成常规与智能体统计。
    #[test]
    fn session_turn_stats_groups_usage_segments_by_owner() {
        let mut stats = SessionTurnStats::default();
        stats.merge_usage_segment_group(&SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::AssistantResponse,
            trigger: None,
            execution_profile: None,
            provider_id: None,
            provider_name_snapshot: None,
            model_id: None,
            model_name_snapshot: None,
            provider_call_count: 1,
            tool_call_count: 0,
            input_tokens: 10,
            output_tokens: 3,
            reasoning_tokens: 0,
            total_tokens: 13,
            cached_input_tokens: Some(4),
            cache_creation_input_tokens: None,
        });
        stats.merge_usage_segment_group(&SessionTurnUsageSegment {
            kind: SessionTurnUsageSegmentKind::SubagentTask,
            trigger: None,
            execution_profile: None,
            provider_id: None,
            provider_name_snapshot: None,
            model_id: None,
            model_name_snapshot: None,
            provider_call_count: 2,
            tool_call_count: 1,
            input_tokens: 20,
            output_tokens: 5,
            reasoning_tokens: 1,
            total_tokens: 26,
            cached_input_tokens: Some(6),
            cache_creation_input_tokens: None,
        });

        assert_eq!(stats.usage_groups.len(), 2);
        assert_eq!(
            stats.usage_groups[0].kind,
            SessionTurnUsageGroupKind::Regular
        );
        assert_eq!(stats.usage_groups[0].total_tokens, 13);
        assert_eq!(
            stats.usage_groups[1].kind,
            SessionTurnUsageGroupKind::Subagent
        );
        assert_eq!(stats.usage_groups[1].provider_call_count, 2);
        assert_eq!(stats.usage_groups[1].total_tokens, 26);
        assert_eq!(stats.usage_groups[1].cached_input_tokens, Some(6));
    }
}
