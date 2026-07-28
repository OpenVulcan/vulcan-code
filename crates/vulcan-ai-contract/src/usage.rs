//! Provider-neutral request usage accounting.
//! 供应商无关的请求用量统计。

use serde::{Deserialize, Serialize};

/// Identifies how one usage value was obtained.
/// 标识一个用量值的取得方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageValueSource {
    /// Reported directly by the provider.
    /// 由供应商直接报告。
    Reported,
    /// Derived from other reported fields.
    /// 从其他已报告字段推导。
    Derived,
    /// Estimated locally.
    /// 在本地估算。
    Estimated,
}

/// Represents a known or unknown token count.
/// 表示一个已知或未知的 Token 数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UsageValue {
    /// Provider did not report enough information.
    /// 供应商未报告足够信息。
    #[default]
    Unknown,
    /// Known token count with provenance.
    /// 带来源的已知 Token 数量。
    Known {
        /// Token count.
        /// Token 数量。
        value: u64,
        /// Value provenance.
        /// 数值来源。
        source: UsageValueSource,
    },
}

/// Carries normalized usage for one provider request.
/// 承载一次供应商请求的归一化用量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestUsage {
    /// Non-classified input tokens when reported.
    /// 已报告但未进一步分类的输入 Token。
    pub input_tokens: UsageValue,
    /// Output tokens.
    /// 输出 Token。
    pub output_tokens: UsageValue,
    /// Reasoning output tokens.
    /// 推理输出 Token。
    pub reasoning_tokens: UsageValue,
    /// Cache-read input tokens.
    /// 缓存读取输入 Token。
    pub cache_read_tokens: UsageValue,
    /// Cache-write input tokens.
    /// 缓存写入输入 Token。
    pub cache_write_tokens: UsageValue,
    /// Five-minute cache-write input tokens.
    /// 五分钟缓存写入输入 Token。
    pub cache_write_5m_tokens: UsageValue,
    /// One-hour cache-write input tokens.
    /// 一小时缓存写入输入 Token。
    pub cache_write_1h_tokens: UsageValue,
    /// Audio input tokens.
    /// 音频输入 Token。
    pub audio_input_tokens: UsageValue,
    /// Audio output tokens.
    /// 音频输出 Token。
    pub audio_output_tokens: UsageValue,
    /// Total tokens.
    /// 总 Token。
    pub total_tokens: UsageValue,
    /// Provider-reported decimal cost string.
    /// 供应商报告的十进制费用字符串。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
}

/// Carries one provider-authoritative count of an input request.
/// 承载一次由供应商权威统计的输入请求 Token 数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputTokenCount {
    /// Total input tokens counted by the provider.
    /// 供应商统计的输入 Token 总数。
    pub total_tokens: u64,
    /// Tokens attributed to cached input when the provider reports them.
    /// 供应商报告时归属于缓存输入的 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// Per-modality token counts for the full input.
    /// 完整输入按模态拆分的 Token 数。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modality_details: Vec<ModalityTokenCount>,
    /// Per-modality token counts for cached input.
    /// 缓存输入按模态拆分的 Token 数。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cached_modality_details: Vec<ModalityTokenCount>,
}

/// Carries one provider-reported modality token count without narrowing future modality names.
/// 承载一个供应商报告的模态 Token 数，并保留未来模态名称的扩展能力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModalityTokenCount {
    /// Exact provider modality identifier.
    /// 供应商返回的精确模态标识。
    pub modality: String,
    /// Token count attributed to this modality.
    /// 归属于该模态的 Token 数。
    pub token_count: u64,
}
