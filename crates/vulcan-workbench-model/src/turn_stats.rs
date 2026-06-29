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
    /// Optional model identifier associated with the segment.
    /// 与该分段关联的可选模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Provider call count represented by this segment.
    /// 该分段代表的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
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
            model_id: None,
            provider_call_count,
            input_tokens,
            output_tokens,
            reasoning_tokens,
            total_tokens,
            cached_input_tokens: usage.and_then(|usage| usage.cached_input_tokens),
            cache_creation_input_tokens: usage.and_then(|usage| usage.cache_creation_input_tokens),
        }
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
    /// Number of provider calls aggregated inside the whole turn.
    /// 在整个轮次内聚合得到的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
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
        assert_eq!(stats.provider_call_count, 1);
        assert_eq!(stats.total_tokens, 14);
    }
}
