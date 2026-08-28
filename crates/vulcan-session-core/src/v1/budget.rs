//! Durable non-sensitive evidence used to calibrate Provider request budgets.
//! 用于校准 Provider 请求预算的持久化非敏感证据。

use serde::{Deserialize, Serialize};

use crate::v1::event::{
    ApiStartedEvent, ApiStatsEvent, ConversationEvent, EventBody, ProviderProjectionProof,
};
use crate::v1::ids::ApiCallId;
use crate::v1::stats::UsageSnapshot;

/// Character-level category weights for one exact Provider-visible request.
/// 一次精确 Provider 可见请求的字符级分类权重。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequestContextBreakdown {
    /// Canonical history and the distinct current prompt.
    /// 规范历史与未包含在历史中的当前提示词。
    pub message_units: u64,
    /// Host-owned tool schemas and tool guidance.
    /// 宿主自有工具 Schema 与工具指引。
    pub system_tool_units: u64,
    /// MCP-discovered tool schemas.
    /// MCP 发现的工具 Schema。
    pub mcp_tool_units: u64,
    /// LuaSkills tool schemas.
    /// LuaSkills 工具 Schema。
    pub skill_units: u64,
    /// System, developer, personality, and output-style instructions.
    /// 系统、开发者、人格与输出风格指令。
    pub system_prompt_units: u64,
    /// Runtime context and remaining structured request data.
    /// 运行时上下文与其余结构化请求数据。
    pub other_units: u64,
}

/// Non-sensitive size and prefix identity of one exact Provider request.
/// 一次精确 Provider 请求的非敏感尺寸与前缀身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequestTextMeasurement {
    /// Unicode scalar count of the canonical request input.
    /// 规范请求输入的 Unicode 标量数量。
    pub character_count: u64,
    /// UTF-8 byte count of the canonical request input.
    /// 规范请求输入的 UTF-8 字节数。
    pub utf8_byte_count: u64,
    /// Explicit structural overhead units used by the estimator.
    /// 估算器使用的显式结构开销单位。
    pub structural_units: u64,
    /// Number of canonical history messages in the request.
    /// 请求中的规范历史消息数量。
    pub history_message_count: u64,
    /// Whether the current prompt is already present in canonical history.
    /// 当前提示词是否已经存在于规范历史中。
    pub prompt_in_history: bool,
    /// SHA-256 identity of the complete canonical history.
    /// 完整规范历史的 SHA-256 身份。
    pub history_fingerprint: String,
    /// SHA-256 identity of request fields that must remain prefix-invariant.
    /// 必须保持前缀不变的请求字段的 SHA-256 身份。
    pub prefix_invariant_fingerprint: String,
    /// Exact category weights used to project calibrated usage.
    /// 用于投影校准后用量的精确分类权重。
    pub context_breakdown: ProviderRequestContextBreakdown,
}

/// Stable non-sensitive evidence derived from one exact canonical Provider request.
/// 从一次精确规范 Provider 请求派生的稳定非敏感证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRequestBudgetEvidence {
    /// SHA-256 identity of the complete canonical request.
    /// 完整规范请求的 SHA-256 身份。
    pub request_assembly_fingerprint: String,
    /// SHA-256 identity of the immutable provider/model/protocol route.
    /// 不可变 Provider、模型与协议路由的 SHA-256 身份。
    pub model_route_fingerprint: String,
    /// SHA-256 identity of the exact exposed tool schemas.
    /// 精确暴露工具 Schema 的 SHA-256 身份。
    pub toolset_fingerprint: String,
    /// SHA-256 identity of system and layered prompt inputs.
    /// 系统与分层提示词输入的 SHA-256 身份。
    pub system_prompt_fingerprint: String,
    /// Exact non-sensitive request measurement.
    /// 精确的非敏感请求测量值。
    pub request_text_measurement: ProviderRequestTextMeasurement,
}

/// Latest completed V1 API that can calibrate one subsequent exact request.
/// 可用于校准后续精确请求的最近一次已完成 V1 API。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUsageBudgetAnchor {
    /// Local physical API identity owning the evidence and usage.
    /// 拥有证据与用量的本地物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Exact V1 projection boundary used by the anchored request.
    /// 锚定请求使用的精确 V1 投影边界。
    pub projection: ProviderProjectionProof,
    /// Persisted request-shape evidence from `api_started`.
    /// 来自 `api_started` 的持久化请求形状证据。
    pub evidence: ProviderRequestBudgetEvidence,
    /// Official usage from the matching terminal `api_stats`.
    /// 来自匹配终态 `api_stats` 的官方用量。
    pub usage: UsageSnapshot,
}

/// Selects the latest completed API that contains enough official input evidence.
/// 选择包含足够官方输入证据的最近一次已完成 API。
///
/// # Parameters
/// 参数
/// `events` is the strictly ordered authoritative V1 event prefix.
/// `events` 是严格有序的权威 V1 事件前缀。
///
/// # Returns
/// 返回值
/// Returns the latest matching `api_started` and `api_stats` pair, or `None` when no usable
/// Provider usage has been reported.
/// 返回最近一组匹配的 `api_started` 与 `api_stats`；尚无可用 Provider 用量时返回 `None`。
pub fn latest_provider_usage_budget_anchor(
    events: &[ConversationEvent],
) -> Option<ProviderUsageBudgetAnchor> {
    events.iter().rev().find_map(|event| {
        let EventBody::ApiStats(stats) = &event.event else {
            return None;
        };
        if stats.usage.input_tokens.is_none() && stats.usage.total_tokens.is_none() {
            return None;
        }
        let started = matching_api_started(events, stats)?;
        Some(ProviderUsageBudgetAnchor {
            api_call_id: started.api_call_id,
            projection: started.projection.clone(),
            evidence: started.request_budget_evidence.clone(),
            usage: stats.usage.clone(),
        })
    })
}

/// Finds the unique start event for one terminal API fact.
/// 查找一条终态 API 事实对应的唯一启动事件。
fn matching_api_started<'a>(
    events: &'a [ConversationEvent],
    stats: &ApiStatsEvent,
) -> Option<&'a ApiStartedEvent> {
    events.iter().find_map(|event| match &event.event {
        EventBody::ApiStarted(started) if started.api_call_id == stats.api_call_id => Some(started),
        _ => None,
    })
}
