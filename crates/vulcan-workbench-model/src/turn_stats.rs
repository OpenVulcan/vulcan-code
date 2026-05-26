//! Per-turn transcript statistics shared by projections and persistence.
//! 供投影与持久化复用的按轮次转录统计模型。

use serde::{Deserialize, Serialize};

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
